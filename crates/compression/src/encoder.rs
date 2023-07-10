//! Compress the body of a response.
use std::io::{Result as IoResult, Write};

#[cfg(feature = "brotli")]
use brotli::CompressorWriter as BrotliEncoder;
use bytes::{Bytes, BytesMut};
#[cfg(feature = "gzip")]
use flate2::write::GzEncoder;
#[cfg(feature = "deflate")]
use flate2::write::ZlibEncoder;
#[cfg(feature = "zstd")]
use zstd::stream::write::Encoder as ZstdEncoder;

use super::{CompressionAlgo, CompressionLevel};

pub(super) struct Writer {
    buf: BytesMut,
}

impl Writer {
    #[allow(dead_code)]
    fn new() -> Writer {
        Writer {
            buf: BytesMut::with_capacity(8192),
        }
    }

    #[allow(dead_code)]
    fn take(&mut self) -> Bytes {
        self.buf.split().freeze()
    }
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

impl CompressionLevel {
    #[cfg(feature = "brotli")]
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

    #[cfg(feature = "deflate")]
    fn into_deflate(self) -> ZlibEncoder<Writer> {
        let compression = match self {
            Self::Fastest => flate2::Compression::fast(),
            Self::Minsize => flate2::Compression::best(),
            Self::Precise(quality) => flate2::Compression::new(quality.min(10)),
            Self::Default => flate2::Compression::fast(),
        };
        ZlibEncoder::new(Writer::new(), compression)
    }

    #[cfg(feature = "gzip")]
    fn into_gzip(self) -> GzEncoder<Writer> {
        let compression = match self {
            Self::Fastest => flate2::Compression::fast(),
            Self::Minsize => flate2::Compression::best(),
            Self::Precise(quality) => flate2::Compression::new(quality.min(10)),
            Self::Default => flate2::Compression::fast(),
        };
        GzEncoder::new(Writer::new(), compression)
    }

    #[cfg(feature = "zstd")]
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
    #[cfg(feature = "brotli")]
    Brotli(Box<BrotliEncoder<Writer>>),
    #[cfg(feature = "deflate")]
    Deflate(ZlibEncoder<Writer>),
    #[cfg(feature = "gzip")]
    Gzip(GzEncoder<Writer>),
    #[cfg(feature = "zstd")]
    Zstd(ZstdEncoder<'static, Writer>),
}

impl Encoder {
    #[allow(unused_variables)]
    pub(super) fn new(algo: CompressionAlgo, level: CompressionLevel) -> Self {
        match algo {
            #[cfg(feature = "brotli")]
            CompressionAlgo::Brotli => Self::Brotli(Box::new(level.into_brotli())),
            #[cfg(feature = "deflate")]
            CompressionAlgo::Deflate => Self::Deflate(level.into_deflate()),
            #[cfg(feature = "gzip")]
            CompressionAlgo::Gzip => Self::Gzip(level.into_gzip()),
            #[cfg(feature = "zstd")]
            CompressionAlgo::Zstd => Self::Zstd(level.into_zstd()),
        }
    }
    #[inline]
    pub(super) fn take(&mut self) -> IoResult<Bytes> {
        match *self {
            #[cfg(feature = "brotli")]
            Self::Brotli(ref mut encoder) => {
                encoder.flush()?;
                Ok(encoder.get_mut().take())
            }
            #[cfg(feature = "deflate")]
            Self::Deflate(ref mut encoder) => {
                encoder.flush()?;
                Ok(encoder.get_mut().take())
            }
            #[cfg(feature = "gzip")]
            Self::Gzip(ref mut encoder) => {
                encoder.flush()?;
                Ok(encoder.get_mut().take())
            }
            #[cfg(feature = "zstd")]
            Self::Zstd(ref mut encoder) => {
                encoder.flush()?;
                Ok(encoder.get_mut().take())
            }
        }
    }

    pub(super) fn finish(self) -> IoResult<Bytes> {
        match self {
            #[cfg(feature = "brotli")]
            Self::Brotli(mut encoder) => match encoder.flush() {
                Ok(()) => Ok(encoder.into_inner().buf.freeze()),
                Err(err) => Err(err),
            },
            #[cfg(feature = "deflate")]
            Self::Deflate(encoder) => match encoder.finish() {
                Ok(writer) => Ok(writer.buf.freeze()),
                Err(err) => Err(err),
            },
            #[cfg(feature = "gzip")]
            Self::Gzip(encoder) => match encoder.finish() {
                Ok(writer) => Ok(writer.buf.freeze()),
                Err(err) => Err(err),
            },
            #[cfg(feature = "zstd")]
            Self::Zstd(encoder) => match encoder.finish() {
                Ok(writer) => Ok(writer.buf.freeze()),
                Err(err) => Err(err),
            },
        }
    }

    #[allow(unused_variables)]
    pub(super) fn write(&mut self, data: &[u8]) -> IoResult<()> {
        match *self {
            #[cfg(feature = "brotli")]
            Self::Brotli(ref mut encoder) => encoder.write_all(data),
            #[cfg(feature = "deflate")]
            Self::Deflate(ref mut encoder) => encoder.write_all(data),
            #[cfg(feature = "gzip")]
            Self::Gzip(ref mut encoder) => encoder.write_all(data),
            #[cfg(feature = "zstd")]
            Self::Zstd(ref mut encoder) => encoder.write_all(data),
        }
    }
}
