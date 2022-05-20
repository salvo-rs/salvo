use std::convert::TryInto;
use std::fs;
use std::io::{copy, Result as IoResult, Seek, SeekFrom, Write, BufWriter};
use serde::ser::Serialize;
use serde_json::ser::to_writer;


/// A request body containing UTF-8-encoded text
#[derive(Debug, Clone)]
pub struct Text<B>(pub B);

impl<B: AsRef<str>> Body for Text<B> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.as_ref().len().try_into().unwrap();
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        writer.write_all(self.0.as_ref().as_bytes())
    }
}

/// A request body containing binary data
#[derive(Debug, Clone)]
pub struct Bytes<B>(pub B);

impl<B: AsRef<[u8]>> Body for Bytes<B> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.as_ref().len().try_into().unwrap();
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        writer.write_all(self.0.as_ref())
    }
}

/// A request body backed by a local file
#[derive(Debug)]
pub struct File(pub fs::File);

impl Body for File {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.seek(SeekFrom::End(0))?;
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        self.0.seek(SeekFrom::Start(0))?;
        copy(&mut self.0, &mut writer)?;
        Ok(())
    }
}

pub(crate) struct ChunkedWriter<W>(pub W);

impl<W: Write> ChunkedWriter<W> {
    pub fn close(mut self) -> IoResult<()> {
        self.0.write_all(b"0\r\n\r\n")
    }
}

impl<W: Write> Write for ChunkedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        write!(self.0, "{:x}\r\n", buf.len())?;
        self.0.write_all(buf)?;
        write!(self.0, "\r\n")?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
    }
}


    /// A request body for streaming out JSON
    #[derive(Debug, Clone)]
    pub struct Json<B>(pub B);

    impl<B: Serialize> Body for Json<B> {
        fn kind(&mut self) -> IoResult<BodyKind> {
            Ok(BodyKind::Chunked)
        }

        fn write<W: Write>(&mut self, writer: W) -> IoResult<()> {
            let mut writer = BufWriter::new(writer);
            to_writer(&mut writer, &self.0)?;
            writer.flush()?;
            Ok(())
        }
    }