//! Compress the body of a response.
use std::io::{self, Error as IoError, Write};

use brotli::CompressorWriter as BrotliEncoder;
use bytes::{Bytes, BytesMut};
use flate2::write::{GzEncoder, ZlibEncoder};
use zstd::stream::write::Encoder as ZstdEncoder;

use super::{CompressionAlgo, CompressionLevel};

pub(super) struct Writer {
    buf: BytesMut,
}

impl Writer {
    fn new() -> Writer {
        Writer {
            buf: BytesMut::with_capacity(8192),
        }
    }

    fn take(&mut self) -> Bytes {
        self.buf.split().freeze()
    }
}

impl io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl CompressionLevel {
    fn into_brotli(self) -> BrotliEncoder<Writer> {
        let quality = match self {
            Self::Fastest => 0,
            Self::Minsize => 11,
            Self::Precise(quality) => quality.min(11),
            Self::Default => 0,
        };
        BrotliEncoder::new(
            Writer::new(),
            32 * 1024, // 32 KiB buffer
            quality,   // BROTLI_PARAM_QUALITY
            22,        // BROTLI_PARAM_LGWIN
        )
    }

    fn into_gzip(self) -> GzEncoder<Writer> {
        let compression = match self {
            Self::Fastest => flate2::Compression::fast(),
            Self::Minsize => flate2::Compression::best(),
            Self::Precise(quality) => flate2::Compression::new(quality.min(10)),
            Self::Default => flate2::Compression::fast(),
        };
        GzEncoder::new(Writer::new(), compression)
    }

    fn into_deflate(self) -> ZlibEncoder<Writer> {
        let compression = match self {
            Self::Fastest => flate2::Compression::fast(),
            Self::Minsize => flate2::Compression::best(),
            Self::Precise(quality) => flate2::Compression::new(quality.min(10)),
            Self::Default => flate2::Compression::fast(),
        };
        ZlibEncoder::new(Writer::new(), compression)
    }

    fn into_zstd(self) -> ZstdEncoder<'static, Writer> {
        let quality = match self {
            Self::Fastest => 1,
            Self::Minsize => 21,
            Self::Precise(quality) => quality.min(21) as i32,
            Self::Default => 1,
        };
        ZstdEncoder::new(Writer::new(), quality).unwrap()
    }
}

pub(super) enum Encoder {
    Deflate(ZlibEncoder<Writer>),
    Gzip(GzEncoder<Writer>),
    Brotli(Box<BrotliEncoder<Writer>>),
    Zstd(ZstdEncoder<'static, Writer>),
}

impl Encoder {
    pub(super) fn new(algo: CompressionAlgo, level: CompressionLevel) -> Self {
        match algo {
            CompressionAlgo::Deflate => Self::Deflate(level.into_deflate()),
            CompressionAlgo::Gzip => Self::Gzip(level.into_gzip()),
            CompressionAlgo::Brotli => Self::Brotli(Box::new(level.into_brotli())),
            CompressionAlgo::Zstd => Self::Zstd(level.into_zstd()),
        }
    }
    #[inline]
    pub(super) fn take(&mut self) -> Bytes {
        match *self {
            Self::Brotli(ref mut encoder) => encoder.get_mut().take(),
            Self::Deflate(ref mut encoder) => encoder.get_mut().take(),
            Self::Gzip(ref mut encoder) => encoder.get_mut().take(),
            Self::Zstd(ref mut encoder) => encoder.get_mut().take(),
        }
    }

    pub(super) fn finish(self) -> Result<Bytes, IoError> {
        match self {
            Self::Brotli(mut encoder) => match encoder.flush() {
                Ok(()) => Ok(encoder.into_inner().buf.freeze()),
                Err(err) => Err(err),
            },
            Self::Gzip(encoder) => match encoder.finish() {
                Ok(writer) => Ok(writer.buf.freeze()),
                Err(err) => Err(err),
            },
            Self::Deflate(encoder) => match encoder.finish() {
                Ok(writer) => Ok(writer.buf.freeze()),
                Err(err) => Err(err),
            },
            Self::Zstd(encoder) => match encoder.finish() {
                Ok(writer) => Ok(writer.buf.freeze()),
                Err(err) => Err(err),
            },
        }
    }

    pub(super) fn write(&mut self, data: &[u8]) -> Result<(), IoError> {
        match *self {
            Self::Brotli(ref mut encoder) => encoder.write_all(data),
            Self::Gzip(ref mut encoder) => encoder.write_all(data),
            Self::Deflate(ref mut encoder) => encoder.write_all(data),
            Self::Zstd(ref mut encoder) => encoder.write_all(data),
        }
    }
}
