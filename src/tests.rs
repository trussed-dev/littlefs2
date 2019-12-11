use generic_array::typenum::consts::{
    self,
    // U1,
    // U8,
};
use crate::{
    // Buffers,
    // File,
    // FileCache,
    // LittleFsAllocation,
    LittleFs,
    error::Result,
    storage,
};

const ERASE_VALUE: u8 = 0xFF;
const STORAGE_SIZE: usize = 131072; // 128KiB = 1024*128

struct RamStorage {
    buf: [u8; STORAGE_SIZE],
}

impl Default for RamStorage {
    fn default() -> Self {
        RamStorage {
            buf: [ERASE_VALUE; STORAGE_SIZE],
        }
    }
}

impl storage::Storage for RamStorage {
    const READ_SIZE: usize = 1;
    const WRITE_SIZE: usize = 32;
    type BLOCK_SIZE = consts::U128;
    type CACHE_SIZE = consts::U32;
    type LOOKAHEADWORDS_SIZE = consts::U1;
    const BLOCK_COUNT: usize = STORAGE_SIZE / 128;

    fn read(&self, off: usize, buf: &mut [u8]) -> Result<usize> {
        // println!("reading {}B from {}", buf.len(), off);
        assert!(buf.len() % Self::READ_SIZE == 0);
        for i in 0..buf.len() {
            if off + i >= self.buf.len() {
                break;
            }
            buf[i] = self.buf[off + i];
        }
        // println!("read was fine, {}", buf.len());
        Ok(buf.len())
    }

    fn write(&mut self, off: usize, data: &[u8]) -> Result<usize> {
        assert!(data.len() % Self::WRITE_SIZE == 0);
        // println!("writing {}B to {}", data.len(), off);
        // println!("namely: {:?}", data);
        for i in 0..data.len() {
            if off + i >= self.buf.len() {
                break;
            }
            self.buf[off + i] = data[i];
        }
        Ok(data.len())
    }

    fn erase(&mut self, off: usize, len: usize) -> Result<usize> {
        // println!("erasing {}B from {}", len, off);
        for byte in &mut self.buf[off..off + len] {
            *byte = ERASE_VALUE;
        }
        Ok(len)
    }
}

#[test]
fn test_format() {
    let mut storage = RamStorage::default();

    let mut alloc = LittleFs::allocate();
    let mut lfs = LittleFs::new_at(&mut alloc, &mut storage);

    assert!(lfs.mount(&mut storage).is_err());

    lfs.format(&mut storage).unwrap();
    lfs.mount(&mut storage).unwrap();

    // let alloc = LittleFs::<_, RamStorage, >::allocate();

    // let mut buffers: Buffers<RamStorage> = Buffers {
    //     read: Default::default(),
    //     write: Default::default(),
    //     lookahead: Default::default(),
    // };
    // // state (lfs::lfs)
    // let lfs = match LittleFs::try_mount(&mut storage, &mut buffers) {
    //     Ok(lfs) => lfs,
    //     Err(_) => {
    //         println!("failed at first");
    //         LittleFs::try_format(&mut storage, &mut buffers).unwrap();
    //         LittleFs::try_mount(&mut storage, &mut buffers).unwrap()
    //     },
    // };
    // lfs.unmount().unwrap();

    // // need to get rid of these annotations again somehow
    // // let mut cache = FileCache::<RamStorage>::new();
    // let mut cache = FileCache::new();
    // let file = File::<RamStorage>::open(&mut cache);

    // // how long does this live?
    // let file2 = File::<RamStorage>::open(&mut FileCache::new());

    // println!("write buf {:p}", lfs.buffers.write.as_ref());
    // println!("littlefs {:p}", &lfs);

    // let (storage, result) = lfs.unmount();
    // result.unwrap();

    // let mut lfs = match LittleFs::try_mount(storage) {
    //     Ok(lfs) => lfs,
    //     Err((_, error)) => { panic!("{:?}", &error); }
    // };
}

