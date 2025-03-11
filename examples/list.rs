use std::{
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
};

use littlefs2::{
    driver::Storage,
    fs::{Allocation, FileType, Filesystem},
    io::{Error, Result},
    object_safe::DynFilesystem,
    path::{Path, PathBuf},
};

use clap::Parser;

#[derive(Parser)]
struct Args {
    path: String,
    block_size: usize,
    #[arg(short, long)]
    write_size: Option<usize>,
    #[arg(short, long)]
    read_size: Option<usize>,
    #[arg(short, long)]
    cache_size: Option<usize>,
    #[arg(short, long)]
    lookahead_size: Option<usize>,
    #[arg(short, long)]
    block_count: Option<usize>,
    #[arg(short, long)]
    show_hex: bool,
}

const BLOCK_COUNT: usize = 288;
const BLOCK_SIZE: usize = 256;

fn main() {
    let args = Args::parse();
    let file = File::open(&args.path).expect("failed to open file");
    let metadata = file.metadata().expect("failed to query metadata");

    let actual_len = usize::try_from(metadata.len()).unwrap();

    if let Some(block_count) = args.block_count {
        assert_eq!(actual_len, args.block_size * block_count);
    }
    assert_eq!(actual_len % args.block_size, 0);
    let block_count = actual_len / args.block_size;

    let mut s = FileStorage {
        file,
        len: actual_len,
        read_size: args.read_size.unwrap_or(args.block_size),
        write_size: args.write_size.unwrap_or(args.block_size),
        cache_size: args.cache_size.unwrap_or(args.block_size),
        lookahead_size: args.lookahead_size.unwrap_or(1),
        block_count,
        block_size: args.block_size,
    };
    let mut alloc = Allocation::new(&s);
    let fs = Filesystem::mount(&mut alloc, &mut s).expect("failed to mount filesystem");

    let available_blocks = fs.available_blocks().unwrap();
    println!("actual_len:       {actual_len}");
    println!("available_blocks: {available_blocks}");
    println!();

    let path = PathBuf::new();
    list(&fs, &path, args.show_hex);
}

fn list(fs: &dyn DynFilesystem, path: &Path, show_hex: bool) {
    fs.read_dir_and_then(path, &mut |iter| {
        for entry in iter {
            let entry = entry.unwrap();
            match entry.file_type() {
                FileType::File => {
                    println!("F {}", entry.path());
                    if show_hex {
                        let bytes: heapless::Vec<u8, 4096> = fs.read(entry.path()).unwrap();
                        println!("  {}", hex::encode_upper(&bytes));
                    }
                }
                FileType::Dir => match entry.file_name().as_str() {
                    "." => (),
                    ".." => (),
                    _ => {
                        list(fs, entry.path(), show_hex);
                    }
                },
            }
        }
        Ok(())
    })
    .unwrap()
}

struct FileStorage {
    file: File,
    len: usize,
    read_size: usize,
    write_size: usize,
    block_size: usize,
    block_count: usize,
    cache_size: usize,
    lookahead_size: usize,
}

impl Storage for FileStorage {
    type CACHE_BUFFER = Vec<u8>;
    type LOOKAHEAD_BUFFER = Vec<u8>;

    fn read_size(&self) -> usize {
        self.read_size
    }
    fn write_size(&self) -> usize {
        self.write_size
    }
    fn block_size(&self) -> usize {
        self.block_size
    }
    fn block_count(&self) -> usize {
        self.block_count
    }

    fn cache_size(&self) -> usize {
        self.cache_size
    }

    fn lookahead_size(&self) -> usize {
        self.lookahead_size
    }

    fn read(&mut self, off: usize, buf: &mut [u8]) -> Result<usize> {
        assert!(off + buf.len() <= BLOCK_SIZE * BLOCK_COUNT);
        if off >= self.len {
            // blocks that are not in the file are assumed to be empty
            buf.iter_mut().for_each(|byte| *byte = 0);
            Ok(buf.len())
        } else {
            self.file
                .seek(SeekFrom::Start(off.try_into().unwrap()))
                .map_err(|_| Error::IO)?;
            self.file.read(buf).map_err(|_| Error::IO)
        }
    }

    fn write(&mut self, _off: usize, _data: &[u8]) -> Result<usize> {
        unimplemented!("read-only filesystem");
    }

    fn erase(&mut self, _off: usize, _len: usize) -> Result<usize> {
        unimplemented!("read-only filesystem");
    }
}
