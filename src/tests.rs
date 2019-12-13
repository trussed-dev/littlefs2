use generic_array::typenum::consts;

use crate::{
    fs::{
        File,
        Filesystem,
        OpenOptions,
        SeekFrom,
    },
    io::{
        Error,
        Result,
        Read,
        Write,
        Seek,
    },
    traits,
};

ram_storage!(
    name=OtherRamStorage,
    trait=traits::Storage,
    erase_value=0xff,
    read_size=1,
    write_size=32,
    cache_size_ty=consts::U32,
    block_size_ty=consts::U256,
    block_size=256,
    block_count=512,
    lookaheadwords_size_ty=consts::U1,
    filename_max_plus_one_ty=consts::U256,
    path_max_plus_one_ty=consts::U256,
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
    filename_max_plus_one_ty=consts::U256,
    path_max_plus_one_ty=consts::U256,
);

#[test]
fn test_format() {
    let mut storage = OtherRamStorage::default();
    let mut alloc = Filesystem::allocate();

    // should fail: FS is not formatted
    assert!(Filesystem::mount(&mut alloc, &mut storage).contains_err(&Error::CorruptFile));
    // should succeed
    assert!(Filesystem::format(&mut alloc, &mut storage).is_ok());
    // should succeed now that storage is formatted
    let fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();
    // check there are no segfaults
    fs.unmount(&mut storage).unwrap();
}

#[test]
fn test_create() {
    let mut storage = RamStorage::default();
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut alloc, &mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    // file does not exist yet, can't open for reading
    assert_eq!(
        File::open(
            "/test_open.txt",
            &mut alloc, &mut fs, &mut storage
        ).map(drop).unwrap_err(), // "real" contains_err is experimental
        Error::NoSuchEntry
    );

    fs.create_dir("/tmp", &mut storage).unwrap();

    // TODO: make previous allocation reusable
    let mut alloc = File::allocate();
    // can create new files
    let mut file = File::create(
        "/tmp/test_open.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    // can write to files
    assert!(file.write(&mut fs, &mut storage, &[0u8, 1, 2]).unwrap() == 3);
    file.sync(&mut fs, &mut storage).unwrap();
    file.close(&mut fs, &mut storage).unwrap();

    // directory is `DirNotEmpty`
    assert_eq!(fs.remove("/tmp", &mut storage).unwrap_err(), Error::DirNotEmpty);

    let metadata = fs.metadata("/tmp", &mut storage).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(0, metadata.len());

    // can move files
    fs.rename("/tmp/test_open.txt", "moved.txt", &mut storage).unwrap();

    let metadata = fs.metadata("/moved.txt", &mut storage).unwrap();
    assert!(metadata.is_file());
    assert_eq!(3, metadata.len());

    fs.remove("/tmp/../tmp/.", &mut storage).unwrap();

    // can read from existing files
    let mut alloc = File::allocate();
    let mut file = File::open(
        "/moved.txt",
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
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut alloc, &mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

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

#[test]
fn test_fancy_open() {
    let mut storage = RamStorage::default();
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut alloc, &mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open("test_fancy_open.txt", &mut alloc, &mut fs, &mut storage)
        .unwrap();

    file.write(&mut fs, &mut storage, b"hello world").unwrap();
    assert_eq!(file.len(&mut fs).unwrap(), 11);

    // don't need to sync in this case
    // file.sync(&mut fs, &mut storage).unwrap();

    file.seek(&mut fs, &mut storage, SeekFrom::Start(6)).unwrap();

    let mut buf = [0u8; 5];
    file.read(&mut fs, &mut storage, &mut buf).unwrap();
    file.close(&mut fs, &mut storage).unwrap();
    fs.unmount(&mut storage).unwrap();

    assert_eq!(&buf, b"world");
}

// These are some tests that ensure our type constructions
// actually do what we intend them to do.
// Since dev-features cannot be optional, trybuild is not `no_std`,
// and we want to actually test `no_std`...
#[test]
#[cfg(feature = "ui-tests")]
fn test_api_safety() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*-fail.rs");
    t.pass("tests/ui/*-pass.rs");
}
