use generic_array::typenum::consts;

use crate::{
    error::Result,
    file::File,
    LittleFs,
    traits,
};

ram_storage!(
    name=RamStorage,
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

#[test]
fn test_format() {
    let mut storage = RamStorage::default();
    let mut alloc = LittleFs::allocate();

    assert!(LittleFs::mount(&mut alloc, &mut storage).is_err());
    assert!(LittleFs::format(&mut alloc, &mut storage).is_ok());

    let fs = match LittleFs::mount(&mut alloc, &mut storage) {
        Ok(fs) => fs,
        Err(_) => unreachable!("this is not supposed to happen!"),
    };

    fs.unmount(&mut storage).unwrap();
}

#[test]
fn test_create() {
    let mut storage = RamStorage::default();

    let mut alloc = LittleFs::allocate();
    LittleFs::format(&mut alloc, &mut storage).unwrap();

    let mut fs = match LittleFs::mount(&mut alloc, &mut storage) {
        Ok(fs) => fs,
        Err(_) => unreachable!("this is not supposed to happen!"),
    };

    let mut alloc = File::allocate();
    let mut file = File::create(
        "/test_open.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.sync(&mut fs, &mut storage).unwrap();
    file.close(&mut fs, &mut storage).unwrap();

    fs.rename("test_open.txt", "moved.txt", &mut storage).unwrap();

    let mut alloc = File::allocate();
    File::open(
        "moved.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();

    fs.unmount(&mut storage).unwrap();
}
