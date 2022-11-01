use std::task::{Context, Poll};

use bytes::{Buf, BufMut as _, Bytes};
use futures_util::{future, ready};
use quic::RecvStream;

use crate::{
    buf::BufList,
    error::{Code, ErrorLevel},
    frame::FrameStream,
    proto::{
        coding::{BufExt, Decode as _, Encode},
        frame::Frame,
        stream::StreamType,
        varint::VarInt,
    },
    quic::{self, SendStream},
    Error,
};

#[inline]
pub(crate) async fn write<S, D, B>(stream: &mut S, data: D) -> Result<(), Error>
where
    S: SendStream<B>,
    D: Into<WriteBuf<B>>,
    B: Buf,
{
    stream.send_data(data)?;
    future::poll_fn(|cx| stream.poll_ready(cx)).await?;

    Ok(())
}

const WRITE_BUF_ENCODE_SIZE: usize = StreamType::MAX_ENCODED_SIZE + Frame::MAX_ENCODED_SIZE;

/// Wrap frames to encode their header on the stack before sending them on the wire
///
/// Implements `Buf` so wire data is seamlessly available for transport layer transmits:
/// `Buf::chunk()` will yield the encoded header, then the payload. For unidirectional streams,
/// this type makes it possible to prefix wire data with the `StreamType`.
///
/// Conveying frames as `Into<WriteBuf>` makes it possible to encode only when generating wire-format
/// data is necessary (say, in `quic::SendStream::send_data`). It also has a public API ergonomy
/// advantage: `WriteBuf` doesn't have to appear in public associated types. On the other hand,
/// QUIC implementers have to call `into()`, which will encode the header in `Self::buf`.
pub struct WriteBuf<B>
where
    B: Buf,
{
    buf: [u8; WRITE_BUF_ENCODE_SIZE],
    len: usize,
    pos: usize,
    frame: Option<Frame<B>>,
}

impl<B> WriteBuf<B>
where
    B: Buf,
{
    fn encode_stream_type(&mut self, ty: StreamType) {
        let mut buf_mut = &mut self.buf[self.len..];
        ty.encode(&mut buf_mut);
        self.len = WRITE_BUF_ENCODE_SIZE - buf_mut.remaining_mut();
    }

    fn encode_frame_header(&mut self) {
        if let Some(frame) = self.frame.as_ref() {
            let mut buf_mut = &mut self.buf[self.len..];
            frame.encode(&mut buf_mut);
            self.len = WRITE_BUF_ENCODE_SIZE - buf_mut.remaining_mut();
        }
    }
}

impl<B> From<StreamType> for WriteBuf<B>
where
    B: Buf,
{
    fn from(ty: StreamType) -> Self {
        let mut me = Self {
            buf: [0; WRITE_BUF_ENCODE_SIZE],
            len: 0,
            pos: 0,
            frame: None,
        };
        me.encode_stream_type(ty);
        me
    }
}

impl<B> From<Frame<B>> for WriteBuf<B>
where
    B: Buf,
{
    fn from(frame: Frame<B>) -> Self {
        let mut me = Self {
            buf: [0; WRITE_BUF_ENCODE_SIZE],
            len: 0,
            pos: 0,
            frame: Some(frame),
        };
        me.encode_frame_header();
        me
    }
}

impl<B> From<(StreamType, Frame<B>)> for WriteBuf<B>
where
    B: Buf,
{
    fn from(ty_stream: (StreamType, Frame<B>)) -> Self {
        let (ty, frame) = ty_stream;
        let mut me = Self {
            buf: [0; WRITE_BUF_ENCODE_SIZE],
            len: 0,
            pos: 0,
            frame: Some(frame),
        };
        me.encode_stream_type(ty);
        me.encode_frame_header();
        me
    }
}

impl<B> Buf for WriteBuf<B>
where
    B: Buf,
{
    fn remaining(&self) -> usize {
        self.len - self.pos
            + self
                .frame
                .as_ref()
                .and_then(|f| f.payload())
                .map_or(0, |x| x.remaining())
    }

    fn chunk(&self) -> &[u8] {
        if self.len - self.pos > 0 {
            &self.buf[self.pos..self.len]
        } else if let Some(payload) = self.frame.as_ref().and_then(|f| f.payload()) {
            payload.chunk()
        } else {
            &[]
        }
    }

    fn advance(&mut self, mut cnt: usize) {
        let remaining_header = self.len - self.pos;
        if remaining_header > 0 {
            let advanced = usize::min(cnt, remaining_header);
            self.pos += advanced;
            cnt -= advanced;
        }

        if let Some(payload) = self.frame.as_mut().and_then(|f| f.payload_mut()) {
            payload.advance(cnt);
        }
    }
}

pub(super) enum AcceptedRecvStream<S, B>
where
    S: quic::RecvStream,
{
    Control(FrameStream<S, B>),
    Push(u64, FrameStream<S, B>),
    Encoder(S),
    Decoder(S),
    Reserved,
}

pub(super) struct AcceptRecvStream<S>
where
    S: quic::RecvStream,
{
    stream: S,
    ty: Option<StreamType>,
    push_id: Option<u64>,
    buf: BufList<Bytes>,
    expected: Option<usize>,
}

impl<S> AcceptRecvStream<S>
where
    S: RecvStream,
{
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            ty: None,
            push_id: None,
            buf: BufList::new(),
            expected: None,
        }
    }

    pub fn into_stream<B>(self) -> Result<AcceptedRecvStream<S, B>, Error> {
        Ok(match self.ty.expect("Stream type not resolved yet") {
            StreamType::CONTROL => AcceptedRecvStream::Control(FrameStream::with_bufs(self.stream, self.buf)),
            StreamType::PUSH => AcceptedRecvStream::Push(
                self.push_id.expect("Push ID not resolved yet"),
                FrameStream::with_bufs(self.stream, self.buf),
            ),
            StreamType::ENCODER => AcceptedRecvStream::Encoder(self.stream),
            StreamType::DECODER => AcceptedRecvStream::Decoder(self.stream),
            t if t.value() > 0x21 && (t.value() - 0x21) % 0x1f == 0 => AcceptedRecvStream::Reserved,

            //= https://www.rfc-editor.org/rfc/rfc9114#section-6.2
            //# Recipients of unknown stream types MUST
            //# either abort reading of the stream or discard incoming data without
            //# further processing.

            //= https://www.rfc-editor.org/rfc/rfc9114#section-6.2
            //# If reading is aborted, the recipient SHOULD use
            //# the H3_STREAM_CREATION_ERROR error code or a reserved error code
            //# (Section 8.1).

            //= https://www.rfc-editor.org/rfc/rfc9114#section-6.2
            //= type=implication
            //# The recipient MUST NOT consider unknown stream types
            //# to be a connection error of any kind.
            t => {
                return Err(Code::H3_STREAM_CREATION_ERROR.with_reason(
                    format!("unknown stream type 0x{:x}", t.value()),
                    crate::error::ErrorLevel::ConnectionError,
                ))
            }
        })
    }

    pub fn poll_type(&mut self, cx: &mut Context) -> Poll<Result<(), Error>> {
        loop {
            match (self.ty.as_ref(), self.push_id) {
                // When accepting a Push stream, we want to parse two VarInts: [StreamType, PUSH_ID]
                (Some(&StreamType::PUSH), Some(_)) | (Some(_), _) => return Poll::Ready(Ok(())),
                _ => (),
            }

            match ready!(self.stream.poll_data(cx))? {
                Some(mut b) => self.buf.push_bytes(&mut b),
                None => {
                    return Poll::Ready(Err(Code::H3_STREAM_CREATION_ERROR
                        .with_reason("Stream closed before type received", ErrorLevel::ConnectionError)));
                }
            };

            if self.expected.is_none() && self.buf.remaining() >= 1 {
                self.expected = Some(VarInt::encoded_size(self.buf.chunk()[0]));
            }

            if let Some(expected) = self.expected {
                if self.buf.remaining() < expected {
                    continue;
                }
            } else {
                continue;
            }

            if self.ty.is_none() {
                // Parse StreamType
                self.ty = Some(StreamType::decode(&mut self.buf).map_err(|_| {
                    Code::H3_INTERNAL_ERROR
                        .with_reason("Unexpected end parsing stream type", ErrorLevel::ConnectionError)
                })?);
                // Get the next VarInt for PUSH_ID on the next iteration
                self.expected = None;
            } else {
                // Parse PUSH_ID
                self.push_id = Some(self.buf.get_var().map_err(|_| {
                    Code::H3_INTERNAL_ERROR
                        .with_reason("Unexpected end parsing stream type", ErrorLevel::ConnectionError)
                })?);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::stream::StreamId;

    #[test]
    fn write_buf_encode_streamtype() {
        let wbuf = WriteBuf::<Bytes>::from(StreamType::ENCODER);

        assert_eq!(wbuf.chunk(), b"\x02");
        assert_eq!(wbuf.len, 1);
    }

    #[test]
    fn write_buf_encode_frame() {
        let wbuf = WriteBuf::<Bytes>::from(Frame::Goaway(StreamId(2)));

        assert_eq!(wbuf.chunk(), b"\x07\x01\x02");
        assert_eq!(wbuf.len, 3);
    }

    #[test]
    fn write_buf_encode_streamtype_then_frame() {
        let wbuf = WriteBuf::<Bytes>::from((StreamType::ENCODER, Frame::Goaway(StreamId(2))));

        assert_eq!(wbuf.chunk(), b"\x02\x07\x01\x02");
    }

    #[test]
    fn write_buf_advances() {
        let mut wbuf = WriteBuf::<Bytes>::from((StreamType::ENCODER, Frame::Data(Bytes::from("hey"))));

        assert_eq!(wbuf.chunk(), b"\x02\x00\x03");
        wbuf.advance(3);
        assert_eq!(wbuf.remaining(), 3);
        assert_eq!(wbuf.chunk(), b"hey");
        wbuf.advance(2);
        assert_eq!(wbuf.chunk(), b"y");
        wbuf.advance(1);
        assert_eq!(wbuf.remaining(), 0);
    }

    #[test]
    fn write_buf_advance_jumps_header_and_payload_start() {
        let mut wbuf = WriteBuf::<Bytes>::from((StreamType::ENCODER, Frame::Data(Bytes::from("hey"))));

        wbuf.advance(4);
        assert_eq!(wbuf.chunk(), b"ey");
    }
}
