//! QUIC Transport implementation with Quinn
//!
//! This module implements QUIC traits with Quinn.
#![deny(missing_docs)]

use std::{
    convert::TryInto,
    fmt::{self, Display},
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
};

use bytes::{Buf, Bytes};
use futures_util::future::FutureExt as _;
use futures_util::io::AsyncWrite as _;
use futures_util::ready;
use futures_util::stream::StreamExt as _;

pub use quinn::{
    self, crypto::Session, Endpoint, IncomingBiStreams, IncomingUniStreams, NewConnection, OpenBi,
    OpenUni, VarInt, WriteError,
};

use h3::quic::{self, Error, StreamId, WriteBuf};

/// A QUIC connection backed by Quinn
///
/// Implements a [`quic::Connection`] backed by a [`quinn::Connection`].
pub struct Connection {
    conn: quinn::Connection,
    incoming_bi: IncomingBiStreams,
    opening_bi: Option<OpenBi>,
    incoming_uni: IncomingUniStreams,
    opening_uni: Option<OpenUni>,
}

impl Connection {
    /// Create a [`Connection`] from a [`quinn::NewConnection`]
    pub fn new(new_conn: NewConnection) -> Self {
        let NewConnection {
            uni_streams,
            bi_streams,
            connection,
            ..
        } = new_conn;

        Self {
            conn: connection,
            incoming_bi: bi_streams,
            opening_bi: None,
            incoming_uni: uni_streams,
            opening_uni: None,
        }
    }
}

/// The error type for [`Connection`]
///
/// Wraps reasons a Quinn connection might be lost.
#[derive(Debug)]
pub struct ConnectionError(quinn::ConnectionError);

impl std::error::Error for ConnectionError {}

impl fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for ConnectionError {
    fn is_timeout(&self) -> bool {
        matches!(self.0, quinn::ConnectionError::TimedOut)
    }

    fn err_code(&self) -> Option<u64> {
        match self.0 {
            quinn::ConnectionError::ApplicationClosed(quinn_proto::ApplicationClose {
                error_code,
                ..
            }) => Some(error_code.into_inner()),
            _ => None,
        }
    }
}

impl From<quinn::ConnectionError> for ConnectionError {
    fn from(e: quinn::ConnectionError) -> Self {
        Self(e)
    }
}

impl<B> quic::Connection<B> for Connection
where
    B: Buf,
{
    type SendStream = SendStream<B>;
    type RecvStream = RecvStream;
    type BidiStream = BidiStream<B>;
    type OpenStreams = OpenStreams;
    type Error = ConnectionError;

    fn poll_accept_bidi(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::BidiStream>, Self::Error>> {
        let (send, recv) = match ready!(self.incoming_bi.next().poll_unpin(cx)) {
            Some(x) => x?,
            None => return Poll::Ready(Ok(None)),
        };
        Poll::Ready(Ok(Some(Self::BidiStream {
            send: Self::SendStream::new(send),
            recv: Self::RecvStream::new(recv),
        })))
    }

    fn poll_accept_recv(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::RecvStream>, Self::Error>> {
        let recv = match ready!(self.incoming_uni.poll_next_unpin(cx)) {
            Some(x) => x?,
            None => return Poll::Ready(Ok(None)),
        };
        Poll::Ready(Ok(Some(Self::RecvStream::new(recv))))
    }

    fn poll_open_bidi(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::BidiStream, Self::Error>> {
        if self.opening_bi.is_none() {
            self.opening_bi = Some(self.conn.open_bi());
        }

        let (send, recv) = ready!(self.opening_bi.as_mut().unwrap().poll_unpin(cx))?;
        Poll::Ready(Ok(Self::BidiStream {
            send: Self::SendStream::new(send),
            recv: Self::RecvStream::new(recv),
        }))
    }

    fn poll_open_send(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>> {
        if self.opening_uni.is_none() {
            self.opening_uni = Some(self.conn.open_uni());
        }

        let send = ready!(self.opening_uni.as_mut().unwrap().poll_unpin(cx))?;
        Poll::Ready(Ok(Self::SendStream::new(send)))
    }

    fn opener(&self) -> Self::OpenStreams {
        OpenStreams {
            conn: self.conn.clone(),
            opening_bi: None,
            opening_uni: None,
        }
    }

    fn close(&mut self, code: h3::error::Code, reason: &[u8]) {
        self.conn.close(
            VarInt::from_u64(code.value()).expect("error code VarInt"),
            reason,
        );
    }
}

/// Stream opener backed by a Quinn connection
///
/// Implements [`quic::OpenStreams`] using [`quinn::Connection`],
/// [`quinn::OpenBi`], [`quinn::OpenUni`].
pub struct OpenStreams {
    conn: quinn::Connection,
    opening_bi: Option<OpenBi>,
    opening_uni: Option<OpenUni>,
}

impl<B> quic::OpenStreams<B> for OpenStreams
where
    B: Buf,
{
    type RecvStream = RecvStream;
    type SendStream = SendStream<B>;
    type BidiStream = BidiStream<B>;
    type Error = ConnectionError;

    fn poll_open_bidi(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::BidiStream, Self::Error>> {
        if self.opening_bi.is_none() {
            self.opening_bi = Some(self.conn.open_bi());
        }

        let (send, recv) = ready!(self.opening_bi.as_mut().unwrap().poll_unpin(cx))?;
        Poll::Ready(Ok(Self::BidiStream {
            send: Self::SendStream::new(send),
            recv: Self::RecvStream::new(recv),
        }))
    }

    fn poll_open_send(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Self::SendStream, Self::Error>> {
        if self.opening_uni.is_none() {
            self.opening_uni = Some(self.conn.open_uni());
        }

        let send = ready!(self.opening_uni.as_mut().unwrap().poll_unpin(cx))?;
        Poll::Ready(Ok(Self::SendStream::new(send)))
    }

    fn close(&mut self, code: h3::error::Code, reason: &[u8]) {
        self.conn.close(
            VarInt::from_u64(code.value()).expect("error code VarInt"),
            reason,
        );
    }
}

impl Clone for OpenStreams {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            opening_bi: None,
            opening_uni: None,
        }
    }
}

/// Quinn-backed bidirectional stream
///
/// Implements [`quic::BidiStream`] which allows the stream to be split
/// into two structs each implementing one direction.
pub struct BidiStream<B>
where
    B: Buf,
{
    send: SendStream<B>,
    recv: RecvStream,
}

impl<B> quic::BidiStream<B> for BidiStream<B>
where
    B: Buf,
{
    type SendStream = SendStream<B>;
    type RecvStream = RecvStream;

    fn split(self) -> (Self::SendStream, Self::RecvStream) {
        (self.send, self.recv)
    }
}

impl<B> quic::RecvStream for BidiStream<B>
where
    B: Buf,
{
    type Buf = Bytes;
    type Error = ReadError;

    fn poll_data(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::Buf>, Self::Error>> {
        self.recv.poll_data(cx)
    }

    fn stop_sending(&mut self, error_code: u64) {
        self.recv.stop_sending(error_code)
    }
}

impl<B> quic::SendStream<B> for BidiStream<B>
where
    B: Buf,
{
    type Error = SendStreamError;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send.poll_ready(cx)
    }

    fn poll_finish(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.send.poll_finish(cx)
    }

    fn reset(&mut self, reset_code: u64) {
        self.send.reset(reset_code)
    }

    fn send_data<D: Into<WriteBuf<B>>>(&mut self, data: D) -> Result<(), Self::Error> {
        self.send.send_data(data)
    }

    fn id(&self) -> StreamId {
        self.send.id()
    }
}

/// Quinn-backed receive stream
///
/// Implements a [`quic::RecvStream`] backed by a [`quinn::RecvStream`].
pub struct RecvStream {
    stream: quinn::RecvStream,
}

impl RecvStream {
    fn new(stream: quinn::RecvStream) -> Self {
        Self { stream }
    }
}

impl quic::RecvStream for RecvStream {
    type Buf = Bytes;
    type Error = ReadError;

    fn poll_data(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Result<Option<Self::Buf>, Self::Error>> {
        Poll::Ready(Ok(ready!(self
            .stream
            .read_chunk(usize::MAX, true)
            .poll_unpin(cx))?
        .map(|c| (c.bytes))))
    }

    fn stop_sending(&mut self, error_code: u64) {
        let _ = self
            .stream
            .stop(VarInt::from_u64(error_code).expect("invalid error_code"));
    }
}

/// The error type for [`RecvStream`]
///
/// Wraps errors that occur when reading from a receive stream.
#[derive(Debug)]
pub struct ReadError(quinn::ReadError);

impl std::error::Error for ReadError {}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<ReadError> for Arc<dyn Error> {
    fn from(e: ReadError) -> Self {
        Arc::new(e)
    }
}

impl From<quinn::ReadError> for ReadError {
    fn from(e: quinn::ReadError) -> Self {
        Self(e)
    }
}

impl Error for ReadError {
    fn is_timeout(&self) -> bool {
        matches!(
            self.0,
            quinn::ReadError::ConnectionLost(quinn::ConnectionError::TimedOut)
        )
    }

    fn err_code(&self) -> Option<u64> {
        match self.0 {
            quinn::ReadError::ConnectionLost(quinn::ConnectionError::ApplicationClosed(
                quinn_proto::ApplicationClose { error_code, .. },
            )) => Some(error_code.into_inner()),
            quinn::ReadError::Reset(error_code) => Some(error_code.into_inner()),
            _ => None,
        }
    }
}

/// Quinn-backed send stream
///
/// Implements a [`quic::SendStream`] backed by a [`quinn::SendStream`].
pub struct SendStream<B: Buf> {
    stream: quinn::SendStream,
    writing: Option<WriteBuf<B>>,
}

impl<B> SendStream<B>
where
    B: Buf,
{
    fn new(stream: quinn::SendStream) -> SendStream<B> {
        Self {
            stream,
            writing: None,
        }
    }
}

impl<B> quic::SendStream<B> for SendStream<B>
where
    B: Buf,
{
    type Error = SendStreamError;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Some(ref mut data) = self.writing {
            while data.has_remaining() {
                match ready!(Pin::new(&mut self.stream).poll_write(cx, data.chunk())) {
                    Ok(cnt) => data.advance(cnt),
                    Err(err) => {
                        // We are forced to use AsyncWrite for now because we cannot store
                        // the result of a call to:
                        // quinn::send_stream::write<'a>(&'a mut self, buf: &'a [u8]) -> Write<'a, S>.
                        //
                        // This is why we have to unpack the error from io::Error below. This should not
                        // panic as long as quinn's AsyncWrite impl doesn't change.
                        return Poll::Ready(Err(SendStreamError::Write(
                            err.into_inner()
                                .expect("write stream returned an empty error")
                                .downcast_ref::<WriteError>()
                                .expect(
                                    "write stream returned an error which type is not WriteError",
                                )
                                .clone(),
                        )));
                    }
                }
            }
        }
        self.writing = None;
        Poll::Ready(Ok(()))
    }

    fn poll_finish(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.stream.finish().poll_unpin(cx).map_err(Into::into)
    }

    fn reset(&mut self, reset_code: u64) {
        let _ = self
            .stream
            .reset(VarInt::from_u64(reset_code).unwrap_or(VarInt::MAX));
    }

    fn send_data<D: Into<WriteBuf<B>>>(&mut self, data: D) -> Result<(), Self::Error> {
        if self.writing.is_some() {
            return Err(Self::Error::NotReady);
        }
        self.writing = Some(data.into());
        Ok(())
    }

    fn id(&self) -> StreamId {
        self.stream.id().0.try_into().expect("invalid stream id")
    }
}

/// The error type for [`SendStream`]
///
/// Wraps errors that can happen writing to or polling a send stream.
#[derive(Debug)]
pub enum SendStreamError {
    /// Errors when writing, wrapping a [`quinn::WriteError`]
    Write(WriteError),
    /// Error when the stream is not ready, because it is still sending
    /// data from a previous call
    NotReady,
}

impl std::error::Error for SendStreamError {}

impl Display for SendStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<WriteError> for SendStreamError {
    fn from(e: WriteError) -> Self {
        Self::Write(e)
    }
}

impl Error for SendStreamError {
    fn is_timeout(&self) -> bool {
        match self {
            Self::Write(quinn::WriteError::ConnectionLost(quinn::ConnectionError::TimedOut)) => {
                true
            }
            _ => false,
        }
    }

    fn err_code(&self) -> Option<u64> {
        match self {
            Self::Write(quinn::WriteError::Stopped(error_code)) => Some(error_code.into_inner()),
            Self::Write(quinn::WriteError::ConnectionLost(
                quinn::ConnectionError::ApplicationClosed(quinn_proto::ApplicationClose {
                    error_code,
                    ..
                }),
            )) => Some(error_code.into_inner()),
            _ => None,
        }
    }
}

impl From<SendStreamError> for Arc<dyn Error> {
    fn from(e: SendStreamError) -> Self {
        Arc::new(e)
    }
}
