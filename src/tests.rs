use generic_array::typenum::consts;

use crate::{
    fs::{
        Attribute,
        File,
        FileWith,
        Filesystem,
        FilesystemWith,
        OpenOptions,
    },
    io::{
        Error,
        Result,
        Read,
        ReadWith,
        Write,
        WriteWith,
        Seek,
        SeekFrom,
    },
    path::Filename,
    driver,
};

ram_storage!(
    name=OtherRamStorage,
    backend=OtherRam,
    trait=driver::Storage,
    erase_value=0xff,
    read_size=1,
    write_size=32,
    cache_size_ty=consts::U32,
    block_size=256,
    block_count=512,
    lookaheadwords_size_ty=consts::U1,
    filename_max_plus_one_ty=consts::U256,
    path_max_plus_one_ty=consts::U256,
    result=Result,
);

ram_storage!(
    name=RamStorage,
    backend=Ram,
    trait=driver::Storage,
    erase_value=0xff,
    read_size=20*5,
    write_size=20*7,
    cache_size_ty=consts::U700,
    block_size=20*35,
    block_count=32,
    lookaheadwords_size_ty=consts::U16,
    filename_max_plus_one_ty=consts::U256,
    path_max_plus_one_ty=consts::U256,
    result=Result,
);

#[test]
fn version() {
    assert_eq!(crate::version().format, (2, 0));
    assert_eq!(crate::version().backend, (2, 1));
}

#[test]
fn format() {
    let mut backend = OtherRam::default();
    let mut storage = OtherRamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();

    // should fail: FS is not formatted
    assert_eq!(
        Filesystem::mount(&mut alloc, &mut storage)
            .map(drop).unwrap_err(),
        Error::Corruption
    );
    // should succeed
    assert!(Filesystem::format(&mut storage).is_ok());
    // should succeed now that storage is formatted
    let fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();
    // check there are no segfaults
    drop(fs);
    drop(storage);
}

// #[macro_use]
// macro_rules! setup_fs {
//     () => {
//         let mut backend = OtherRam::default();
//         let mut storage = OtherRamStorage::new(&mut backend);
//         let mut alloc_fs = Filesystem::allocate();
//         Filesystem::format(&mut alloc_fs, &mut storage).unwrap();
//         let mut fs = Filesystem::mount(&mut alloc_fs, &mut storage).unwrap();
//     }
// }

#[test]
fn borrow_fs_allocation() {
    let mut backend = OtherRam::default();

    let mut storage = OtherRamStorage::new(&mut backend);
    let mut alloc_fs = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let _fs = Filesystem::mount(&mut alloc_fs, &mut storage).unwrap();
    // previous `_fs` is fine as it's masked, due to NLL
    let mut fs = Filesystem::mount(&mut alloc_fs, &mut storage).unwrap();

    let mut alloc_file = File::allocate();
    let _file = File::create("data.bin", &mut alloc_file, &mut fs, &mut storage).unwrap();
    File::create("data.bin", &mut alloc_file, &mut fs, &mut storage).unwrap();

}

#[test]
fn test_fs_with() -> Result<()> {
    let mut backend = OtherRam::default();
    let mut storage = OtherRamStorage::new(&mut backend);
    let mut alloc_fs = Filesystem::allocate();
    Filesystem::format(&mut storage)?;
    let mut fs = FilesystemWith::mount(&mut alloc_fs, &mut storage)?;
    // let mut fs = Filesystem::mount(&mut alloc_fs, &mut storage)?.with(&mut storage);

    assert_eq!(fs.total_blocks(), 512);
    assert_eq!(fs.total_space(), 256*512);
    // superblock
    assert_eq!(fs.available_blocks()?, 512 - 2);
    assert_eq!(fs.available_space()?, 130_560);

    fs.create_dir("/tmp")?;       assert_eq!(fs.available_blocks()?, 512 - 4);
    fs.create_dir("/mnt")?;       assert_eq!(fs.available_blocks()?, 512 - 6);
    fs.rename("tmp", "mnt/tmp")?; assert_eq!(fs.available_blocks()?, 512 - 6);
    fs.remove("/mnt/tmp")?;       assert_eq!(fs.available_blocks()?, 512 - 4);
    fs.remove("/mnt")?;           assert_eq!(fs.available_blocks()?, 512 - 2);

    let mut alloc_file = File::allocate();

    let mut file = FileWith::create("/test_with.txt", &mut alloc_file, &mut fs)?;
    assert!(file.write(&[0u8, 1, 2])? == 3);
    file.sync()?;
    drop(file);

    let mut alloc_file = File::allocate();
    let mut file = FileWith::open("/test_with.txt", &mut alloc_file, &mut fs)?;
    let mut buf = [0u8; 3];
    {
        let fs = file.borrow_filesystem();
        assert_eq!(fs.available_blocks()?, 510);
    }
    assert!(file.read_exact(&mut buf).is_ok());
    assert_eq!(&buf, &[0, 1, 2]);
    drop(file);

    // surprise surprise, inline files!
    assert_eq!(fs.available_blocks()?, 512 - 2);

    Ok(())
}

#[test]
fn test_create() {
    let mut backend = OtherRam::default();
    let mut storage = OtherRamStorage::new(&mut backend);
    let mut alloc_fs = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc_fs, &mut storage).unwrap();

    assert_eq!(fs.total_blocks(), 512);
    assert_eq!(fs.total_space(), 256*512);
    // superblock
    assert_eq!(fs.available_blocks(&mut storage).unwrap(), 512 - 2);
    assert_eq!(fs.available_space(&mut storage).unwrap(), 130_560);

    let mut alloc_file = File::allocate();
    // file does not exist yet, can't open for reading
    assert_eq!(
        File::open(
            "/test_open.txt",
            &mut alloc_file, &mut fs, &mut storage
        ).map(drop).unwrap_err(), // "real" contains_err is experimental
        Error::NoSuchEntry
    );

    fs.create_dir("/tmp", &mut storage).unwrap();
    assert_eq!(fs.available_blocks(&mut storage).unwrap(), 512 - 2 - 2);

    // can create new files
    let mut file = File::create(
        "/tmp/test_open.txt",
        &mut alloc_file, &mut fs, &mut storage,
    ).unwrap();
    // can write to files
    assert!(file.write(&mut fs, &mut storage, &[0u8, 1, 2]).unwrap() == 3);
    file.sync(&mut fs, &mut storage).unwrap();
    // surprise surprise, inline files!
    assert_eq!(fs.available_blocks(&mut storage).unwrap(), 512 - 2 - 2);
    file.close(&mut fs, &mut storage).unwrap();

    // cannot remove non-empty directories
    assert_eq!(fs.remove("/tmp", &mut storage).unwrap_err(), Error::DirNotEmpty);

    let metadata = fs.metadata("/tmp", &mut storage).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.len(), 0);

    // can move files
    fs.rename("/tmp/test_open.txt", "moved.txt", &mut storage).unwrap();
    assert_eq!(fs.available_blocks(&mut storage).unwrap(), 512 - 2 - 2);

    let metadata = fs.metadata("/moved.txt", &mut storage).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 3);

    fs.remove("/tmp/../tmp/.", &mut storage).unwrap();
    assert_eq!(fs.available_blocks(&mut storage).unwrap(), 512 - 2);

    // can read from existing files
    let mut alloc = File::allocate();
    let mut file = File::open(
        "/moved.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();

    assert!(file.len(&mut fs, &mut storage).unwrap() == 3);
    let mut contents: [u8; 3] = Default::default();
    assert!(file.read(&mut fs, &mut storage, &mut contents).unwrap() == 3);
    assert_eq!(contents, [0u8, 1, 2]);
}

#[test]
fn test_unbind() {
    let mut backend = Ram::default();

    {
        let mut storage = RamStorage::new(&mut backend);
        let mut alloc = Filesystem::allocate();
        Filesystem::format(&mut storage).unwrap();
        let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

        let mut alloc = File::allocate();
        let mut file = File::create(
            "test_unbind.txt",
            &mut alloc, &mut fs, &mut storage,
        ).unwrap();
        file.write(&mut fs, &mut storage, b"hello world").unwrap();
        assert_eq!(file.len(&mut fs, &mut storage).unwrap(), 11);
        // w/o sync, won't see data below
        file.sync(&mut fs, &mut storage).unwrap();
    }

    let mut storage = RamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();
    let mut alloc = File::allocate();
    let mut file = File::open(
        "test_unbind.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    let mut buf = <[u8; 11]>::default();
    file.read(&mut fs, &mut storage, &mut buf).unwrap();
    assert_eq!(&buf, b"hello world");
}

#[test]
fn test_seek() {
    let mut backend = Ram::default();
    let mut storage = RamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = File::create(
        "test_seek.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.write(&mut fs, &mut storage, b"hello world").unwrap();
    assert_eq!(file.len(&mut fs, &mut storage).unwrap(), 11);
    // w/o sync, won't see data below
    file.sync(&mut fs, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = File::open(
        "test_seek.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.seek(&mut fs, &mut storage, SeekFrom::End(-5)).unwrap();

    let mut buf = [0u8; 5];
    assert_eq!(file.len(&mut fs, &mut storage).unwrap(), 11);
    file.read(&mut fs, &mut storage, &mut buf).unwrap();
    file.close(&mut fs, &mut storage).unwrap();
    drop(fs);

    assert_eq!(&buf, b"world");
}

#[test]
fn test_file_set_len() {
    let mut backend = Ram::default();
    let mut storage = RamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = File::create(
        "test_set_len.txt",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.write(&mut fs, &mut storage, b"hello littlefs").unwrap();
    assert_eq!(file.len(&mut fs, &mut storage).unwrap(), 14);

    file.set_len(&mut fs, &mut storage, 10).unwrap();
    assert_eq!(file.len(&mut fs, &mut storage).unwrap(), 10);

    // note that:
    // a) "tell" can be implemented as follows,
    // b) truncating a file does not change the cursor position
    assert_eq!(file.seek(&mut fs, &mut storage, SeekFrom::Current(0)).unwrap(), 14);
}

#[test]
fn test_fancy_open() {
    let mut backend = Ram::default();
    let mut storage = RamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open("test_fancy_open.txt", &mut alloc, &mut fs, &mut storage)
        .unwrap();

    file.write(&mut fs, &mut storage, b"hello world").unwrap();
    assert_eq!(file.len(&mut fs, &mut storage).unwrap(), 11);

    // don't need to sync in this case
    // file.sync(&mut fs, &mut storage).unwrap();

    file.seek(&mut fs, &mut storage, SeekFrom::Start(6)).unwrap();

    let mut buf = [0u8; 5];
    file.read(&mut fs, &mut storage, &mut buf).unwrap();
    file.close(&mut fs, &mut storage).unwrap();
    drop(fs);

    assert_eq!(&buf, b"world");
}

#[test]
fn attributes() {
    let mut backend = Ram::default();
    let mut storage = RamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    let mut alloc = File::allocate();
    let mut file = File::create(
        "some.file",
        &mut alloc, &mut fs, &mut storage,
    ).unwrap();
    file.sync(&mut fs, &mut storage).unwrap();

    assert!(fs.attribute("some.file", 37, &mut storage).unwrap().is_none());

    let mut attribute = Attribute::<RamStorage>::new(37);
    attribute.set_data(b"top secret");

    fs.set_attribute("some.file", &attribute, &mut storage).unwrap();
    assert_eq!(
        b"top secret",
        fs.attribute("some.file", 37, &mut storage).unwrap().unwrap().data()
    );

    // // not sure if we should have this method (may be quite expensive)
    // let attributes = unsafe { fs.attributes("some.file", &mut storage).unwrap() };
    // assert!(attributes[37]);
    // assert_eq!(attributes.iter().fold(0, |sum, i| sum + (*i as u8)), 1);

    fs.remove_attribute("some.file", 37, &mut storage).unwrap();
    assert!(fs.attribute("some.file", 37, &mut storage).unwrap().is_none());

    // Directories can have attributes too
    attribute.set_data(b"temporary directory");
    fs.create_dir("/tmp", &mut storage).unwrap();
    fs.set_attribute("/tmp", &attribute, &mut storage).unwrap();
    assert_eq!(
        b"temporary directory",
        fs.attribute("/tmp", 37, &mut storage).unwrap().unwrap().data()
    );
    fs.remove_attribute("/tmp", 37, &mut storage).unwrap();
    assert!(fs.attribute("/tmp", 37, &mut storage).unwrap().is_none());
}

#[test]
fn test_iter_dirs() {
    let mut backend = Ram::default();
    let mut storage = RamStorage::new(&mut backend);
    let mut alloc = Filesystem::allocate();
    Filesystem::format(&mut storage).unwrap();
    let mut fs = Filesystem::mount(&mut alloc, &mut storage).unwrap();

    fs.create_dir("/tmp", &mut storage).unwrap();

    let mut alloc_a = File::allocate();
    let mut file_a = File::create(
        "/tmp/file.a",
        &mut alloc_a, &mut fs, &mut storage,
    ).unwrap();
    file_a.set_len(&mut fs, &mut storage, 37).unwrap();
    file_a.sync(&mut fs, &mut storage).unwrap();

    let mut alloc_b = File::allocate();
    let mut file_b = File::create(
        "/tmp/file.b",
        &mut alloc_b, &mut fs, &mut storage,
    ).unwrap();
    file_b.set_len(&mut fs, &mut storage, 42).unwrap();
    file_b.sync(&mut fs, &mut storage).unwrap();

    let mut read_dir = fs.read_dir("/tmp", &mut storage).unwrap();

    let mut found_files: usize = 0;
    let mut sizes = [0usize; 4];

    // de-sugared `for` loop
    // NB: iterating first gives the special directories `.` and `..` :'-)
    loop {
        match read_dir.next(&mut fs, &mut storage) {
            Some(x) => {
                let x = x.unwrap();
                let i = found_files;
                if i == 0 {
                    assert_eq!(x.file_name(), Filename::new(b"."));
                }
                if i == 1 {
                    assert_eq!(x.file_name(), Filename::new(b".."));
                }
                if i == 2 {
                    assert_eq!(x.file_name(), Filename::new(b"file.a"));
                }
                if i == 3 {
                    assert_eq!(x.file_name(), Filename::new(b"file.b"));
                }
                sizes[found_files] = x.metadata().len();
                found_files += 1;
            },
            None => break,
        }
    }
    assert_eq!(sizes, [0, 0, 37, 42]);
    assert_eq!(found_files, 4);

    // version dealing with an actual iterator
    // TODO: does not release the references properly (can't do this before the above version)
    for (i, entry) in fs
        .read_dir("/tmp", &mut storage)
        .unwrap()
        .with(&mut fs, &mut storage)
        .enumerate()
    {
        let x = entry.unwrap();
        if i == 0 { assert_eq!(x.file_name(), Filename::new(b".")); }
        if i == 1 { assert_eq!(x.file_name(), Filename::new(b"..")); }
        if i == 2 { assert_eq!(x.file_name(), Filename::new(b"file.a")); }
        if i == 3 { assert_eq!(x.file_name(), Filename::new(b"file.b")); }
    }

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
