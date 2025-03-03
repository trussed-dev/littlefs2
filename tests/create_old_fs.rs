use littlefs2::{
    consts,
    fs::Filesystem,
    path,
    path::{Path, PathBuf},
    ram_storage,
};

ram_storage!(
    name = RamStorage,
    backend = Ram,
    erase_value = 0xff,
    read_size = 20 * 5,
    write_size = 20 * 7,
    cache_size_ty = consts::U700,
    block_size = 20 * 35,
    block_count = 32,
    lookahead_size_ty = consts::U16,
    filename_max_plus_one_ty = consts::U256,
    path_max_plus_one_ty = consts::U256,
);

struct FileTest {
    name: &'static Path,
    content: &'static [u8],
}

struct DirTest {
    files: &'static [FileTest],
    sub_dirs: &'static [(&'static Path, DirTest)],
}

struct FsTest {
    root: DirTest,
    name: &'static str,
}

const EMPTY_DIR: FsTest = FsTest {
    name: "empty.bin",
    root: DirTest {
        files: &[],
        sub_dirs: &[],
    },
};

const ROOT_FULL: FsTest = FsTest {
    name: "root.bin",
    root: DirTest {
        files: &[
            FileTest {
                name: path!("test_file.txt"),
                content: b"Test content - test_file.txt",
            },
            FileTest {
                name: path!("test_file2.txt"),
                content: b"Test content - test_file2.txt",
            },
            FileTest {
                name: path!("test_file3.txt"),
                content: b"Test content - test_file3.txt",
            },
        ],
        sub_dirs: &[],
    },
};

const RECURSE: FsTest = FsTest {
    name: "recurse.bin",
    root: DirTest {
        files: ROOT_FULL.root.files,
        sub_dirs: &[
            (
                path!("root1"),
                DirTest {
                    files: &[
                        FileTest {
                            name: path!("test_sub_file.txt"),
                            content: b"Test content - test_sub_file.txt",
                        },
                        FileTest {
                            name: path!("test_sub_file2.txt"),
                            content: b"Test content - test_sub_file2.txt",
                        },
                    ],
                    sub_dirs: &[(
                        path!("sub-dir"),
                        DirTest {
                            files: &[
                                FileTest {
                                    name: path!("test_sub_sub_file.txt"),
                                    content: b"Test content - test_sub_sub_file.txt",
                                },
                                FileTest {
                                    name: path!("test_sub_sub_file2.txt"),
                                    content: b"Test content - test_sub_sub_file2.txt",
                                },
                            ],
                            sub_dirs: &[],
                        },
                    )],
                },
            ),
            (
                path!("root2"),
                DirTest {
                    files: &[],
                    sub_dirs: &[],
                },
            ),
        ],
    },
};

const ALL: &[FsTest] = &[EMPTY_DIR, ROOT_FULL, RECURSE];

fn write_dir(fs: &Filesystem<RamStorage>, dir: &DirTest, current_dir: PathBuf) {
    println!("Writing current_dir: {current_dir}");
    for f in dir.files {
        let mut buf = current_dir.clone();
        buf.push(f.name);
        println!(
            "Writing {}, ({})",
            f.name,
            std::str::from_utf8(f.content).unwrap()
        );
        fs.write(&buf, f.content).unwrap();
    }

    for (name, d) in dir.sub_dirs {
        let mut buf = current_dir.clone();
        buf.push(name);
        fs.create_dir(&buf).unwrap();
        write_dir(fs, d, buf);
    }
}

fn read_dir(fs: &Filesystem<RamStorage>, dir: &DirTest, current_dir: PathBuf) {
    println!("Reading current_dir: {current_dir}");
    for f in dir.files {
        let mut buf = current_dir.clone();
        buf.push(f.name);
        dbg!(&buf);
        let read = fs.read::<1024>(&buf).unwrap();
        assert_eq!(std::str::from_utf8(&read), std::str::from_utf8(f.content));
    }

    for (name, d) in dir.sub_dirs {
        let mut buf = current_dir.clone();
        buf.push(name);
        read_dir(fs, d, buf);
    }
}

#[test]
#[ignore]
fn create() {
    for fs_test in ALL {
        println!("Got to test: {}", fs_test.name);
        let mut backend = Ram::default();
        let mut storage = RamStorage::new(&mut backend);
        Filesystem::format(&mut storage).unwrap();
        Filesystem::mount_and_then(&mut storage, |fs| {
            write_dir(fs, &fs_test.root, PathBuf::new());
            Ok(())
        })
        .unwrap();
        std::fs::write(format!("tests-old-fs/{}", fs_test.name), backend.buf).unwrap();
    }
}

#[test]
fn read() {
    for fs_test in ALL {
        println!("Got to test: {}", fs_test.name);
        let mut backend = Ram::default();
        let buf = std::fs::read(format!("tests-old-fs/{}", fs_test.name)).unwrap();
        backend.buf.copy_from_slice(&buf);
        let mut storage = RamStorage::new(&mut backend);
        Filesystem::mount_and_then(&mut storage, |fs| {
            read_dir(fs, &fs_test.root, PathBuf::new());
            Ok(())
        })
        .unwrap();
    }
}
