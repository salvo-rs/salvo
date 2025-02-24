// Copyright (c) 2018-2020 Sean McArthur
// Licensed under the MIT license http://opensource.org/licenses/MIT
// port from https://github.com/seanmonstar/warp/blob/master/src/filters/sse.rs
//! Middleware for Server-Sent Events (SSE)
//!
//! # Example
//!
//! ```no_run
//! use std::time::Duration;
//! use std::convert::Infallible;
//! use futures_util::stream::iter;
//! use futures_util::Stream;
//!
//! use salvo_core::prelude::*;
//! use salvo_extra::sse::{self, SseEvent};
//!
//! fn sse_events() -> impl Stream<Item = Result<SseEvent, Infallible>> {
//!     iter(vec![
//!         Ok(SseEvent::default().text("unnamed event")),
//!         Ok(
//!             SseEvent::default().name("chat")
//!             .text("chat message")
//!         ),
//!         Ok(
//!             SseEvent::default().id(13.to_string())
//!             .name("chat")
//!             .text("other chat message\nwith next line")
//!             .retry(Duration::from_millis(5000))
//!         )
//!     ])
//! }
//! #[handler]
//! async fn handle(res: &mut Response) {
//!     sse::stream(res, sse_events());
//! }
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::with_path("push-notifications").get(handle);
//!     let accepor = TcpListener::new("127.0.0.1:5800").bind().await;
//!     Server::new(accepor).serve(router).await;
//! }
//! ```
//!
//! Each field already is event which can be sent to client.
//! The events with multiple fields can be created by combining fields using tuples.
//!
//! See also the [EventSource](https://developer.mozilla.org/en-US/docs/Web/API/EventSource) API,
//! which specifies the expected behavior of Server Sent Events.

use serde::Serialize;
use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::future;
use futures_util::stream::{Stream, TryStream, TryStreamExt};
use pin_project::pin_project;
use salvo_core::http::header::{HeaderValue, CACHE_CONTROL, CONTENT_TYPE};
use tokio::time::{self, Sleep};

use salvo_core::http::Response;

/// Server-sent event data type
#[derive(Clone, Debug)]
enum DataType {
    Text(String),
    Json(String),
}
/// SseError
#[derive(Debug)]
pub struct SseError;

impl Display for SseError {
    #[inline]
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "sse error")
    }
}

impl StdError for SseError {}
/// Server-sent event
#[derive(Default, Clone, Debug)]
pub struct SseEvent {
    name: Option<String>,
    id: Option<String>,
    data: Option<DataType>,
    comment: Option<String>,
    retry: Option<Duration>,
}

impl SseEvent {
    /// Sets Server-sent event data.
    #[inline]
    pub fn text<T: Into<String>>(mut self, data: T) -> SseEvent {
        self.data = Some(DataType::Text(data.into()));
        self
    }

    /// Sets Server-sent event data.
    #[inline]
    pub fn json<T: Serialize>(mut self, data: T) -> Result<SseEvent, serde_json::Error> {
        self.data = Some(DataType::Json(serde_json::to_string(&data)?));
        Ok(self)
    }

    /// Sets Server-sent event comment.`
    #[inline]
    pub fn comment<T: Into<String>>(mut self, comment: T) -> SseEvent {
        self.comment = Some(comment.into());
        self
    }

    /// Sets Server-sent event event.
    #[inline]
    pub fn name<T: Into<String>>(mut self, event: T) -> SseEvent {
        self.name = Some(event.into());
        self
    }

    /// Sets Server-sent event retry.
    #[inline]
    pub fn retry(mut self, duration: Duration) -> SseEvent {
        self.retry = Some(duration);
        self
    }

    /// Sets Server-sent event id.
    #[inline]
    pub fn id<T: Into<String>>(mut self, id: T) -> SseEvent {
        self.id = Some(id.into());
        self
    }
}

impl Display for SseEvent {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Some(comment) = &self.comment {
            ":".fmt(f)?;
            comment.fmt(f)?;
            f.write_char('\n')?;
        }

        if let Some(name) = &self.name {
            "event:".fmt(f)?;
            name.fmt(f)?;
            f.write_char('\n')?;
        }

        match &self.data {
            Some(DataType::Text(data)) => {
                for line in data.split('\n') {
                    "data:".fmt(f)?;
                    line.fmt(f)?;
                    f.write_char('\n')?;
                }
            }
            Some(DataType::Json(data)) => {
                "data:".fmt(f)?;
                data.fmt(f)?;
                f.write_char('\n')?;
            }
            None => {}
        }

        if let Some(id) = &self.id {
            "id:".fmt(f)?;
            id.fmt(f)?;
            f.write_char('\n')?;
        }

        if let Some( duration) = &self.retry {
            "retry:".fmt(f)?;

            let secs = duration.as_secs();
            let millis = duration.subsec_millis();

            if secs > 0 {
                // format seconds
                secs.fmt(f)?;

                // pad milliseconds
                if millis < 10 {
                    f.write_str("00")?;
                } else if millis < 100 {
                    f.write_char('0')?;
                }
            }

            // format milliseconds
            millis.fmt(f)?;

            f.write_char('\n')?;
        }

        f.write_char('\n')?;
        Ok(())
    }
}

/// SseKeepAlive
#[allow(missing_debug_implementations)]
#[pin_project]
#[non_exhaustive]
pub struct SseKeepAlive<S> {
    #[pin]
    event_stream: S,
    /// Comment field.
    pub comment: Cow<'static, str>,
    /// Max interval between keep-alive messages.
    pub max_interval: Duration,
    #[pin]
    alive_timer: Sleep,
}

impl<S> SseKeepAlive<S>
where
    S: TryStream<Ok = SseEvent> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    /// Create new `SseKeepAlive`.
    #[inline]
    pub fn new(event_stream: S) -> SseKeepAlive<S> {
        let max_interval = Duration::from_secs(15);
        let alive_timer = time::sleep(max_interval);
        SseKeepAlive {
            event_stream,
            comment: Cow::Borrowed(""),
            max_interval,
            alive_timer,
        }
    }
    /// Customize the interval between keep-alive messages.
    ///
    /// Default is 15 seconds.
    #[inline]
    pub fn max_interval(mut self, time: Duration) -> Self {
        self.max_interval = time;
        self
    }

    /// Customize the text of the keep-alive message.
    ///
    /// Default is an empty comment.
    #[inline]
    pub fn comment(mut self, comment: impl Into<Cow<'static, str>>) -> Self {
        self.comment = comment.into();
        self
    }

    /// Send stream.
    #[inline]
    pub fn stream(self, res: &mut Response) {
        stream(res, self)
    }
}

#[inline]
fn write_response_headers(res: &mut Response) {
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    // Disable response body caching
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
}

/// Send event stream.
#[inline]
pub fn stream<S>(res: &mut Response, event_stream: S)
where
    S: TryStream<Ok = SseEvent> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    write_response_headers(res);
    let body_stream = event_stream
        .map_err(|e| {
            tracing::error!("sse stream error: {}", e);
            SseError
        })
        .into_stream()
        .and_then(|event| future::ready(Ok(event.to_string())));
    res.stream(body_stream)
}

impl<S> Stream for SseKeepAlive<S>
where
    S: TryStream<Ok = SseEvent> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    type Item = Result<SseEvent, SseError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut pin = self.project();
        match pin.event_stream.try_poll_next(cx) {
            Poll::Pending => match Pin::new(&mut pin.alive_timer).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    // restart timer
                    pin.alive_timer.reset(tokio::time::Instant::now() + *pin.max_interval);
                    let event = SseEvent::default().comment(pin.comment.clone());
                    Poll::Ready(Some(Ok(event)))
                }
            },
            Poll::Ready(Some(Ok(event))) => {
                // restart timer
                pin.alive_timer.reset(tokio::time::Instant::now() + *pin.max_interval);
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(e))) => {
                tracing::error!(error = ?e, "sse::keep error");
                Poll::Ready(Some(Err(SseError)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::time::Duration;

    use salvo_core::prelude::*;
    use salvo_core::test::ResponseExt;
    use tokio_stream;

    use super::*;

    #[tokio::test]
    async fn test_sse_data() {
        let event_stream = tokio_stream::iter(vec![
            Ok::<_, Infallible>(SseEvent::default().text("1")),
            Ok::<_, Infallible>(SseEvent::default().text("2")),
        ]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains("data:1") && text.contains("data:2"));
    }

    #[tokio::test]
    async fn test_sse_keep_alive() {
        let event_stream = tokio_stream::iter(vec![Ok::<_, Infallible>(SseEvent::default().text("1"))]);
        let mut res = Response::new();
        SseKeepAlive::new(event_stream)
            .comment("love you")
            .max_interval(Duration::from_secs(1))
            .stream(&mut res);
        let text = res.take_string().await.unwrap();
        assert!(text.contains("data:1"));
    }

    #[tokio::test]
    async fn test_sse_json() {
        #[derive(Serialize, Debug)]
        struct User {
            name: String,
        }

        let event_stream = tokio_stream::iter(vec![SseEvent::default().json(User {
            name: "jobs".to_owned(),
        })]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains(r#"data:{"name":"jobs"}"#));
    }

    #[tokio::test]
    async fn test_sse_comment() {
        let event_stream = tokio_stream::iter(vec![Ok::<_, Infallible>(SseEvent::default().comment("comment"))]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains(":comment"));
    }

    #[tokio::test]
    async fn test_sse_name() {
        let event_stream = tokio_stream::iter(vec![Ok::<_, Infallible>(SseEvent::default().name("evt2"))]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains("event:evt2"));
    }

    #[tokio::test]
    async fn test_sse_retry() {
        let event_stream = tokio_stream::iter(vec![Ok::<_, Infallible>(
            SseEvent::default().retry(std::time::Duration::from_secs_f32(1.0)),
        )]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains("retry:1000"));

        let event_stream = tokio_stream::iter(vec![Ok::<_, Infallible>(
            SseEvent::default().retry(std::time::Duration::from_secs_f32(1.001)),
        )]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains("retry:1001"));
    }

    #[tokio::test]
    async fn test_sse_id() {
        let event_stream = tokio_stream::iter(vec![Ok::<_, Infallible>(SseEvent::default().id("jobs"))]);
        let mut res = Response::new();
        super::stream(&mut res, event_stream);
        let text = res.take_string().await.unwrap();
        assert!(text.contains("id:jobs"));
    }
}
