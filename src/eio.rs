use crate::io;

impl embedded_io::Error for io::Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            io::Error::Success
            | io::Error::Io
            | io::Error::Corruption
            | io::Error::PathNotDir
            | io::Error::PathIsDir
            | io::Error::DirNotEmpty
            | io::Error::FileTooBig
            | io::Error::NoSpace
            | io::Error::NoAttribute
            | io::Error::Unknown(_) => embedded_io::ErrorKind::Other,
            io::Error::EntryAlreadyExisted => embedded_io::ErrorKind::AlreadyExists,
            io::Error::NoSuchEntry => embedded_io::ErrorKind::NotFound,
            io::Error::BadFileDescriptor | io::Error::Invalid | io::Error::FilenameTooLong => {
                embedded_io::ErrorKind::InvalidInput
            }
            io::Error::NoMemory => embedded_io::ErrorKind::OutOfMemory,
        }
    }
}

pub struct Reader<'a, T: io::Read>(pub(crate) &'a T);

impl<'a, T: io::Read> Reader<'a, T> {
    pub fn new(read: &'a T) -> Self {
        Self(read)
    }
}

impl<'a, T: io::Read> embedded_io::ErrorType for Reader<'a, T> {
    type Error = io::Error;
}

impl<'a, T: io::Read> embedded_io::Read for Reader<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf)
    }

    fn read_exact(
        &mut self,
        buf: &mut [u8],
    ) -> Result<(), embedded_io::ReadExactError<Self::Error>> {
        self.0
            .read_exact(buf)
            .map_err(|e| embedded_io::ReadExactError::Other(e))
    }
}

pub struct Writer<'a, T: io::Write>(pub(crate) &'a T);

impl<'a, T: io::Write> Writer<'a, T> {
    pub fn new(write: &'a T) -> Self {
        Self(write)
    }
}

impl<'a, T: io::Write> embedded_io::ErrorType for Writer<'a, T> {
    type Error = io::Error;
}

impl<'a, T: io::Write> embedded_io::Write for Writer<'a, T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.0.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0.write_all(buf)
    }
}
