use std::marker::PhantomData;
use std::task::{Context, Poll};

use bytes::{Buf, Bytes};

use futures_util::ready;
use tracing::trace;

use crate::{
    buf::BufList,
    error::TransportError,
    proto::{
        frame::{self, Frame, PayloadLen},
        stream::StreamId,
    },
    quic::{BidiStream, RecvStream, SendStream},
    stream::WriteBuf,
};

pub struct FrameStream<S, B> {
    stream: S,
    bufs: BufList<Bytes>,
    decoder: FrameDecoder,
    remaining_data: usize,
    /// Set to true when `stream` reaches the end.
    is_eos: bool,
    _phantom_buffer: PhantomData<B>,
}

impl<S, B> FrameStream<S, B> {
    pub fn new(stream: S) -> Self {
        Self::with_bufs(stream, BufList::new())
    }

    pub(crate) fn with_bufs(stream: S, bufs: BufList<Bytes>) -> Self {
        Self {
            stream,
            bufs,
            decoder: FrameDecoder::default(),
            remaining_data: 0,
            is_eos: false,
            _phantom_buffer: PhantomData,
        }
    }
}

impl<S, B> FrameStream<S, B>
where
    S: RecvStream,
{
    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Frame<PayloadLen>>, FrameStreamError>> {
        assert!(
            self.remaining_data == 0,
            "There is still data to read, please call poll_data() until it returns None."
        );

        loop {
            let end = self.try_recv(cx)?;

            return match self.decoder.decode(&mut self.bufs)? {
                Some(Frame::Data(PayloadLen(len))) => {
                    self.remaining_data = len;
                    Poll::Ready(Ok(Some(Frame::Data(PayloadLen(len)))))
                }
                Some(frame) => Poll::Ready(Ok(Some(frame))),
                None => match end {
                    // Received a chunk but frame is incomplete, poll until we get `Pending`.
                    Poll::Ready(false) => continue,
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(true) => {
                        if self.bufs.has_remaining() {
                            // Reached the end of receive stream, but there is still some data:
                            // The frame is incomplete.
                            Poll::Ready(Err(FrameStreamError::UnexpectedEnd))
                        } else {
                            Poll::Ready(Ok(None))
                        }
                    }
                },
            };
        }
    }

    pub fn poll_data(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<impl Buf>, FrameStreamError>> {
        if self.remaining_data == 0 {
            return Poll::Ready(Ok(None));
        };

        let end = ready!(self.try_recv(cx))?;
        let data = self.bufs.take_chunk(self.remaining_data);

        match (data, end) {
            (None, true) => Poll::Ready(Ok(None)),
            (None, false) => Poll::Pending,
            (Some(d), true) if d.remaining() < self.remaining_data && !self.bufs.has_remaining() => {
                Poll::Ready(Err(FrameStreamError::UnexpectedEnd))
            }
            (Some(d), _) => {
                self.remaining_data -= d.remaining();
                Poll::Ready(Ok(Some(d)))
            }
        }
    }

    pub(crate) fn stop_sending(&mut self, error_code: crate::error::Code) {
        self.stream.stop_sending(error_code.into());
    }

    pub(crate) fn has_data(&self) -> bool {
        self.remaining_data != 0
    }

    pub(crate) fn is_eos(&self) -> bool {
        self.is_eos && !self.bufs.has_remaining()
    }

    fn try_recv(&mut self, cx: &mut Context<'_>) -> Poll<Result<bool, FrameStreamError>> {
        if self.is_eos {
            return Poll::Ready(Ok(true));
        }
        match self.stream.poll_data(cx) {
            Poll::Ready(Err(e)) => Poll::Ready(Err(FrameStreamError::Quic(e.into()))),
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(None)) => {
                self.is_eos = true;
                Poll::Ready(Ok(true))
            }
            Poll::Ready(Ok(Some(mut d))) => {
                self.bufs.push_bytes(&mut d);
                Poll::Ready(Ok(false))
            }
        }
    }
}

impl<T, B> SendStream<B> for FrameStream<T, B>
where
    T: SendStream<B>,
    B: Buf,
{
    type Error = <T as SendStream<B>>::Error;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.stream.poll_ready(cx)
    }

    fn send_data<D: Into<WriteBuf<B>>>(&mut self, data: D) -> Result<(), Self::Error> {
        self.stream.send_data(data)
    }

    fn poll_finish(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.stream.poll_finish(cx)
    }

    fn reset(&mut self, reset_code: u64) {
        self.stream.reset(reset_code)
    }

    fn id(&self) -> StreamId {
        self.stream.id()
    }
}

impl<S, B> FrameStream<S, B>
where
    S: BidiStream<B>,
    B: Buf,
{
    pub(crate) fn split(self) -> (FrameStream<S::SendStream, B>, FrameStream<S::RecvStream, B>) {
        let (send, recv) = self.stream.split();
        (
            FrameStream {
                stream: send,
                bufs: BufList::new(),
                decoder: FrameDecoder::default(),
                remaining_data: 0,
                is_eos: false,
                _phantom_buffer: PhantomData,
            },
            FrameStream {
                stream: recv,
                bufs: self.bufs,
                decoder: self.decoder,
                remaining_data: self.remaining_data,
                is_eos: self.is_eos,
                _phantom_buffer: PhantomData,
            },
        )
    }
}

#[derive(Default)]
pub struct FrameDecoder {
    expected: Option<usize>,
}

impl FrameDecoder {
    fn decode<B: Buf>(&mut self, src: &mut BufList<B>) -> Result<Option<Frame<PayloadLen>>, FrameStreamError> {
        // Decode in a loop since we ignore unknown frames, and there may be
        // other frames already in our BufList.
        loop {
            if !src.has_remaining() {
                return Ok(None);
            }

            if let Some(min) = self.expected {
                if src.remaining() < min {
                    return Ok(None);
                }
            }

            let (pos, decoded) = {
                let mut cur = src.cursor();
                let decoded = Frame::decode(&mut cur);
                (cur.position(), decoded)
            };

            match decoded {
                Err(frame::FrameError::UnknownFrame(ty)) => {
                    trace!("ignore unknown frame type {:#x}", ty);
                    src.advance(pos);
                    self.expected = None;
                    continue;
                }
                Err(frame::FrameError::Incomplete(min)) => {
                    self.expected = Some(min);
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
                Ok(frame) => {
                    src.advance(pos);
                    self.expected = None;
                    return Ok(Some(frame));
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum FrameStreamError {
    Proto(frame::FrameError),
    Quic(TransportError),
    UnexpectedEnd,
}

impl From<frame::FrameError> for FrameStreamError {
    fn from(err: frame::FrameError) -> Self {
        FrameStreamError::Proto(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_matches::assert_matches;
    use bytes::{BufMut, BytesMut};
    use futures_util::future::poll_fn;
    use std::{collections::VecDeque, fmt, sync::Arc};

    use crate::{
        proto::{coding::Encode, frame::FrameType, varint::VarInt},
        quic,
    };

    // Decoder

    #[test]
    fn one_frame() {
        let mut buf = BytesMut::with_capacity(16);
        Frame::headers(&b"salut"[..]).encode_with_payload(&mut buf);
        let mut buf = BufList::from(buf);

        let mut decoder = FrameDecoder::default();
        assert_matches!(decoder.decode(&mut buf), Ok(Some(Frame::Headers(_))));
    }

    #[test]
    fn incomplete_frame() {
        let frame = Frame::headers(&b"salut"[..]);

        let mut buf = BytesMut::with_capacity(16);
        frame.encode(&mut buf);
        buf.truncate(buf.len() - 1);
        let mut buf = BufList::from(buf);

        let mut decoder = FrameDecoder::default();
        assert_matches!(decoder.decode(&mut buf), Ok(None));
    }

    #[test]
    fn header_spread_multiple_buf() {
        let mut buf = BytesMut::with_capacity(16);
        Frame::headers(&b"salut"[..]).encode_with_payload(&mut buf);
        let mut buf_list = BufList::new();
        // Cut buffer between type and length
        buf_list.push(&buf[..1]);
        buf_list.push(&buf[1..]);

        let mut decoder = FrameDecoder::default();
        assert_matches!(decoder.decode(&mut buf_list), Ok(Some(Frame::Headers(_))));
    }

    #[test]
    fn varint_spread_multiple_buf() {
        let mut buf = BytesMut::with_capacity(16);
        Frame::headers("salut".repeat(1024)).encode_with_payload(&mut buf);

        let mut buf_list = BufList::new();
        // Cut buffer in the middle of length's varint
        buf_list.push(&buf[..2]);
        buf_list.push(&buf[2..]);

        let mut decoder = FrameDecoder::default();
        assert_matches!(decoder.decode(&mut buf_list), Ok(Some(Frame::Headers(_))));
    }

    #[test]
    fn two_frames_then_incomplete() {
        let mut buf = BytesMut::with_capacity(64);
        Frame::headers(&b"header"[..]).encode_with_payload(&mut buf);
        Frame::Data(&b"body"[..]).encode_with_payload(&mut buf);
        Frame::headers(&b"trailer"[..]).encode_with_payload(&mut buf);

        buf.truncate(buf.len() - 1);
        let mut buf = BufList::from(buf);

        let mut decoder = FrameDecoder::default();
        assert_matches!(decoder.decode(&mut buf), Ok(Some(Frame::Headers(_))));
        assert_matches!(decoder.decode(&mut buf), Ok(Some(Frame::Data(PayloadLen(4)))));
        assert_matches!(decoder.decode(&mut buf), Ok(None));
    }

    // FrameStream

    macro_rules! assert_poll_matches {
        ($poll_fn:expr, $match:pat) => {
            assert_matches!(
                poll_fn($poll_fn).await,
                $match
            );
        };
        ($poll_fn:expr, $match:pat if $cond:expr ) => {
            assert_matches!(
                poll_fn($poll_fn).await,
                $match if $cond
            );
        }
    }

    #[tokio::test]
    async fn poll_full_request() {
        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        Frame::headers(&b"header"[..]).encode_with_payload(&mut buf);
        Frame::Data(&b"body"[..]).encode_with_payload(&mut buf);
        Frame::headers(&b"trailer"[..]).encode_with_payload(&mut buf);
        recv.chunk(buf.freeze());

        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Headers(_))));
        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Data(PayloadLen(4)))));
        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Ok(Some(b)) if b.remaining() == 4
        );
        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Headers(_))));
    }

    #[tokio::test]
    async fn poll_next_incomplete_frame() {
        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        Frame::headers(&b"header"[..]).encode_with_payload(&mut buf);
        let mut buf = buf.freeze();
        recv.chunk(buf.split_to(buf.len() - 1));
        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Err(FrameStreamError::UnexpectedEnd));
    }

    #[tokio::test]
    #[should_panic(expected = "There is still data to read, please call poll_data() until it returns None")]
    async fn poll_next_reamining_data() {
        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        FrameType::DATA.encode(&mut buf);
        VarInt::from(4u32).encode(&mut buf);
        recv.chunk(buf.freeze());
        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Data(PayloadLen(4)))));

        // There is still data to consume, poll_next should panic
        let _ = poll_fn(|mut cx| stream.poll_next(&mut cx)).await;
    }

    #[tokio::test]
    async fn poll_data_split() {
        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        // Body is split into two bufs
        Frame::Data(Bytes::from("body")).encode_with_payload(&mut buf);

        let mut buf = buf.freeze();
        recv.chunk(buf.split_to(buf.len() - 2));
        recv.chunk(buf);
        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        // We get the total size of data about to be received
        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Data(PayloadLen(4)))));

        // Then we get parts of body, chunked as they arrived
        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Ok(Some(b)) if b.remaining() == 2
        );
        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Ok(Some(b)) if b.remaining() == 2
        );
    }

    #[tokio::test]
    async fn poll_data_unexpected_end() {
        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        // Truncated body
        FrameType::DATA.encode(&mut buf);
        VarInt::from(4u32).encode(&mut buf);
        buf.put_slice(&b"b"[..]);
        recv.chunk(buf.freeze());
        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Data(PayloadLen(4)))));
        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Err(FrameStreamError::UnexpectedEnd)
        );
    }

    #[tokio::test]
    async fn poll_data_ignores_unknown_frames() {
        use crate::proto::varint::BufMutExt as _;

        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        // grease a lil
        crate::proto::frame::FrameType::grease().encode(&mut buf);
        buf.write_var(0);

        // grease with some data
        crate::proto::frame::FrameType::grease().encode(&mut buf);
        buf.write_var(6);
        buf.put_slice(b"grease");

        // Body
        Frame::Data(Bytes::from("body")).encode_with_payload(&mut buf);

        recv.chunk(buf.freeze());
        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Data(PayloadLen(4)))));
        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Ok(Some(b)) if &*b == b"body"
        );
    }

    #[tokio::test]
    async fn poll_data_eos_but_buffered_data() {
        let mut recv = FakeRecv::default();
        let mut buf = BytesMut::with_capacity(64);

        FrameType::DATA.encode(&mut buf);
        VarInt::from(4u32).encode(&mut buf);
        buf.put_slice(&b"bo"[..]);
        recv.chunk(buf.clone().freeze());

        let mut stream: FrameStream<_, ()> = FrameStream::new(recv);

        assert_poll_matches!(|mut cx| stream.poll_next(&mut cx), Ok(Some(Frame::Data(PayloadLen(4)))));

        buf.truncate(0);
        buf.put_slice(&b"dy"[..]);
        stream.bufs.push_bytes(&mut buf.freeze());

        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Ok(Some(b)) if &*b == b"bo"
        );

        assert_poll_matches!(
            |mut cx| to_bytes(stream.poll_data(&mut cx)),
            Ok(Some(b)) if &*b == b"dy"
        );
    }

    // Helpers

    #[derive(Default)]
    struct FakeRecv {
        chunks: VecDeque<Bytes>,
    }

    impl FakeRecv {
        fn chunk(&mut self, buf: Bytes) -> &mut Self {
            self.chunks.push_back(buf.into());
            self
        }
    }

    impl RecvStream for FakeRecv {
        type Buf = Bytes;
        type Error = FakeError;

        fn poll_data(&mut self, _: &mut Context<'_>) -> Poll<Result<Option<Self::Buf>, Self::Error>> {
            Poll::Ready(Ok(self.chunks.pop_front()))
        }

        fn stop_sending(&mut self, _: u64) {
            unimplemented!()
        }
    }

    #[derive(Debug)]
    struct FakeError;

    impl quic::Error for FakeError {
        fn is_timeout(&self) -> bool {
            unimplemented!()
        }

        fn err_code(&self) -> Option<u64> {
            unimplemented!()
        }
    }

    impl std::error::Error for FakeError {}
    impl fmt::Display for FakeError {
        fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
            unimplemented!()
        }
    }

    impl Into<Arc<dyn quic::Error>> for FakeError {
        fn into(self) -> Arc<dyn quic::Error> {
            unimplemented!()
        }
    }

    fn to_bytes(x: Poll<Result<Option<impl Buf>, FrameStreamError>>) -> Poll<Result<Option<Bytes>, FrameStreamError>> {
        x.map(|b| b.map(|b| b.map(|mut b| b.copy_to_bytes(b.remaining()))))
    }
}
