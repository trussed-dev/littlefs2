// TODO: should add another backend that randomly returns less
// data than requested, to emphasize the difference between
// `io::Read::read` and `::read_exact`.
/// A configurable implementation of the Storage trait in memory.
#[macro_export]
macro_rules! ram_storage {
    (

    name=$Name:ident,
    backend=$Backend:ident,
    erase_value=$erase_value:expr,
    read_size=$read_size:expr,
    write_size=$write_size:expr,
    cache_size=$cache_size:expr,
    block_size=$block_size:expr,
    block_count=$block_count:expr,
    lookahead_size=$lookahead_size:expr,

) => {
        pub struct $Backend {
            buf: [u8; $block_size * $block_count],
        }

        impl Default for $Backend {
            fn default() -> Self {
                $Backend {
                    buf: [$erase_value; $block_size * $block_count],
                }
            }
        }

        pub struct $Name<'backend> {
            backend: &'backend mut $Backend,
        }

        impl<'backend> $Name<'backend> {
            const ERASE_VALUE: u8 = $erase_value;
            pub fn new(backend: &'backend mut $Backend) -> Self {
                $Name { backend }
            }
        }

        impl<'backend> $crate::driver::Storage for $Name<'backend> {
            fn read_size(&self) -> usize {
                $read_size
            }
            fn write_size(&self) -> usize {
                $write_size
            }
            fn block_size(&self) -> usize {
                $block_size
            }
            fn cache_size(&self) -> usize {
                $cache_size
            }
            type CACHE_BUFFER = [u8; $cache_size];
            fn block_count(&self) -> usize {
                $block_count
            }

            fn lookahead_size(&self) -> usize {
                $lookahead_size
            }
            type LOOKAHEAD_BUFFER = [u8; $lookahead_size * 8];

            fn read(&mut self, offset: usize, buf: &mut [u8]) -> $crate::io::Result<usize> {
                let read_size: usize = self.read_size();
                debug_assert!(offset % read_size == 0);
                debug_assert!(buf.len() % read_size == 0);
                for (from, to) in self.backend.buf[offset..].iter().zip(buf.iter_mut()) {
                    *to = *from;
                }
                Ok(buf.len())
            }

            fn write(&mut self, offset: usize, data: &[u8]) -> $crate::io::Result<usize> {
                let write_size: usize = self.write_size();
                debug_assert!(offset % write_size == 0);
                debug_assert!(data.len() % write_size == 0);
                for (from, to) in data.iter().zip(self.backend.buf[offset..].iter_mut()) {
                    *to = *from;
                }
                Ok(data.len())
            }

            fn erase(&mut self, offset: usize, len: usize) -> $crate::io::Result<usize> {
                let block_size: usize = self.block_size();
                debug_assert!(offset % block_size == 0);
                debug_assert!(len % block_size == 0);
                for byte in self.backend.buf[offset..offset + len].iter_mut() {
                    *byte = Self::ERASE_VALUE;
                }
                Ok(len)
            }
        }
    };
    ($Name:ident, $Backend:ident, $bytes:expr) => {
        ram_storage!(
            name = $Name,
            backend = $Backend,
            erase_value = 0xff,
            read_size = 1,
            write_size = 1,
            cache_size = 32,
            block_size = 128,
            block_count = $bytes / 128,
            lookahead_size = 1,
        );
    };
    (tiny) => {
        ram_storage!(
            name = RamStorage,
            backend = Ram,
            erase_value = 0xff,
            read_size = 32,
            write_size = 32,
            cache_size = 32,
            block_size = 128,
            block_count = 8,
            lookahead_size = 1,
        );
    };
    (large) => {
        ram_storage!(
            name = RamStorage,
            backend = Ram,
            erase_value = 0xff,
            read_size = 32,
            write_size = 32,
            cache_size = 32,
            block_size = 256,
            block_count = 512,
            lookahead_size = 4,
        );
    };
}

#[macro_export]
macro_rules! const_ram_storage {
    (

    name=$Name:ident,
    erase_value=$erase_value:expr,
    read_size=$read_size:expr,
    write_size=$write_size:expr,
    cache_size=$cache_size:expr,
    block_size=$block_size:expr,
    block_count=$block_count:expr,
    lookahead_size=$lookahead_size:expr,

) => {
        pub struct $Name {
            buf: [u8; $block_size * $block_count],
        }

        impl $Name {
            const ERASE_VALUE: u8 = $erase_value;
            pub const fn new() -> Self {
                // Self::default()
                Self {
                    buf: [$erase_value; $block_size * $block_count],
                }
            }
        }

        impl Default for $Name {
            fn default() -> Self {
                Self {
                    buf: [$erase_value; $block_size * $block_count],
                }
            }
        }

        impl $crate::driver::Storage for $Name {
            fn read_size(&self) -> usize {
                $read_size
            }

            fn write_size(&self) -> usize {
                $write_size
            }

            fn cache_size(&self) -> usize {
                $cache_size
            }
            type CACHE_BUFFER = [u8; $cache_size];
            fn block_size(&self) -> usize {
                $block_size
            }
            fn block_count(&self) -> usize {
                $block_count
            }

            fn lookahead_size(&self) -> usize {
                $lookahead_size
            }
            type LOOKAHEAD_BUFFER = [u8; $lookahead_size * 8];

            fn read(&mut self, offset: usize, buf: &mut [u8]) -> $crate::io::Result<usize> {
                let read_size = self.read_size();
                debug_assert!(offset % read_size == 0);
                debug_assert!(buf.len() % read_size == 0);
                for (from, to) in self.buf[offset..].iter().zip(buf.iter_mut()) {
                    *to = *from;
                }
                Ok(buf.len())
            }

            fn write(&mut self, offset: usize, data: &[u8]) -> $crate::io::Result<usize> {
                let write_size = self.write_size();
                debug_assert!(offset % write_size == 0);
                debug_assert!(data.len() % write_size == 0);
                for (from, to) in data.iter().zip(self.buf[offset..].iter_mut()) {
                    *to = *from;
                }
                Ok(data.len())
            }

            fn erase(&mut self, offset: usize, len: usize) -> $crate::io::Result<usize> {
                let block_size: usize = self.block_size();
                debug_assert!(offset % block_size == 0);
                debug_assert!(len % block_size == 0);
                for byte in self.buf[offset..offset + len].iter_mut() {
                    *byte = Self::ERASE_VALUE;
                }
                Ok(len)
            }
        }
    };
    ($Name:ident, $bytes:expr) => {
        const_ram_storage!(
            name = $Name,
            erase_value = 0xff,
            read_size = 16,
            write_size = 512,
            cache_size = 512,
            block_size = 512,
            block_count = $bytes / 512,
            lookahead_size = 1,
        );
    };
}
