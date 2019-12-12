use generic_array::typenum::consts;

use crate::{
    error::Result,
    file::File,
    LittleFs,
    SeekFrom,
    traits::{
        self,
        Read,
        Write,
        Seek,
    },
};

ram_storage!(
    name=RamStorageNormal,
    trait=traits::Storage,
    erase_value=0xff,
    read_size=1,
    write_size=32,
    cache_size_ty=consts::U32,
    block_size_ty=consts::U256,
    block_size=256,
    block_count=512,
    lookaheadwords_size_ty=consts::U1,
    filename_max_ty=consts::U255,
);

ram_storage!(
    name=RamStorage,
    trait=traits::Storage,
    erase_value=0xff,
    read_size=20*5,
    write_size=20*7,
    cache_size_ty=consts::U700,
    block_size_ty=consts::U700,
    block_size=20*35,
    block_count=32,
    lookaheadwords_size_ty=consts::U1,
    filename_max_ty=consts::U255,
);

#[test]
fn test_format() {
    let mut storage = RamStorage::default();
    let mut alloc = LittleFs::allocate();

    // should fail: FS is not formatted
    assert!(LittleFs::mount(&mut alloc, &mut storage).is_err());
    // should succeed
    assert!(LittleFs::format(&mut alloc, &mut storage).is_ok());
    // should succeed now that storage is formatted
    let fs = LittleFs::mount(&mut alloc, &mut storage).unwrap();
    // check there are no segfaults
    fs.unmount(&mut storage).unwrap();
}

#[test]
fn test_create() {
    let mut storage = RamStorage::default();
    let mut alloc = LittleFs::allocate();
    LittleFs::format(&mut alloc, &mut storage).unwrap();
    let mut fs = LittleFs::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    // file does not exist yet, can't open for reading
    assert!(File::open(
        "/test_open.txt",
        &mut alloc, &mut fs, &mut storage,
    ).is_err());

    // TODO: make previous allocation reusable
    let mut alloc = File::allocate();
    // can create new files
    let mut file = File::create(
        "/test_open.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    // can write to files
    assert!(file.write(&mut fs, &mut storage, &[0u8, 1, 2]).unwrap() == 3);
    file.sync(&mut fs, &mut storage).unwrap();
    file.close(&mut fs, &mut storage).unwrap();

    // can rename files
    fs.rename("test_open.txt", "moved.txt", &mut storage).unwrap();

    // can read from existing files
    let mut alloc = File::allocate();
    let mut file = File::open(
        "moved.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();

    assert!(file.len(&mut fs).unwrap() == 3);
    let mut contents: [u8; 3] = Default::default();
    assert!(file.read(&mut fs, &mut storage, &mut contents).unwrap() == 3);
    assert_eq!(contents, [0u8, 1, 2]);

    fs.unmount(&mut storage).unwrap();
}

#[test]
fn test_seek() {
    let mut storage = RamStorage::default();
    let mut alloc = LittleFs::allocate();
    LittleFs::format(&mut alloc, &mut storage).unwrap();
    let mut fs = LittleFs::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = File::create(
        "test_seek.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.write(&mut fs, &mut storage, b"hello world").unwrap();
    assert_eq!(file.len(&mut fs).unwrap(), 11);
    // w/o sync, won't see data below
    file.sync(&mut fs, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = File::open(
        "test_seek.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.seek(&mut fs, &mut storage, SeekFrom::End(-5)).unwrap();

    let mut buf = [0u8; 5];
    assert_eq!(file.len(&mut fs).unwrap(), 11);
    file.read(&mut fs, &mut storage, &mut buf).unwrap();
    file.close(&mut fs, &mut storage).unwrap();
    fs.unmount(&mut storage).unwrap();

    assert_eq!(&buf, b"world");
}