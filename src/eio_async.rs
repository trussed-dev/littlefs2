use crate::io;

impl<'a, T: io::Read> embedded_io_async::Read for super::eio::Reader<'a, T> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf)
    }
}

impl<'a, T: io::Write> embedded_io_async::Write for super::eio::Writer<'a, T> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.write(buf)
    }
}
