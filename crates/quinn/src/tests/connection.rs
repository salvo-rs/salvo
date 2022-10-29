use std::{borrow::BorrowMut, time::Duration};

use assert_matches::assert_matches;
use bytes::{Buf, Bytes, BytesMut};
use futures_util::{future, StreamExt};
use http::{Request, Response, StatusCode};

use crate::{
    client::{self, SendRequest},
    connection::ConnectionState,
    error::{Code, Error, Kind},
    proto::{
        coding::Encode as _,
        frame::{Frame, Settings},
        stream::{StreamId, StreamType},
    },
    quic::{self, SendStream},
    server,
};

use super::h3_quinn;
use super::{init_tracing, Pair};

#[tokio::test]
async fn connect() {
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let _ = client::new(pair.client().await).await.expect("client init");
    };

    let server_fut = async {
        let conn = server.next().await;
        let _ = server::Connection::new(conn).await.unwrap();
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn accept_request_end_on_client_close() {
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let _ = client::new(pair.client().await).await.expect("client init");
        // client is dropped, it will send H3_NO_ERROR
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        // Accept returns Ok(None)
        assert!(incoming.accept().await.unwrap().is_none());
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn server_drop_close() {
    let mut pair = Pair::default();
    let mut server = pair.server();

    let server_fut = async {
        let conn = server.next().await;
        let _ = server::Connection::new(conn).await.unwrap();
    };

    let (mut conn, mut send) = client::new(pair.client().await).await.expect("client init");
    let client_fut = async {
        let request_fut = async move {
            let mut request_stream = send
                .send_request(Request::get("http://no.way").body(()).unwrap())
                .await
                .unwrap();
            let response = request_stream.recv_response().await;
            assert_matches!(response.unwrap_err().kind(), Kind::Closed);
        };

        let drive_fut = async {
            let drive = future::poll_fn(|cx| conn.poll_close(cx)).await;
            assert_matches!(drive, Ok(()));
        };
        tokio::select! {biased; _ = request_fut => (), _ = drive_fut => () }
    };
    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn client_close_only_on_last_sender_drop() {
    let mut pair = Pair::default();
    let mut server = pair.server();

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        assert!(incoming.accept().await.unwrap().is_some());
        assert!(incoming.accept().await.unwrap().is_some());
        assert!(incoming.accept().await.unwrap().is_none());
    };

    let client_fut = async {
        let (mut conn, mut send1) = client::new(pair.client().await).await.expect("client init");
        let mut send2 = send1.clone();
        let _ = send1
            .send_request(Request::get("http://no.way").body(()).unwrap())
            .await
            .unwrap()
            .finish()
            .await;
        let _ = send2
            .send_request(Request::get("http://no.way").body(()).unwrap())
            .await
            .unwrap()
            .finish()
            .await;
        drop(send1);
        drop(send2);

        let drive = future::poll_fn(|cx| conn.poll_close(cx)).await;
        assert_matches!(drive, Ok(()));
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn settings_exchange_client() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let (mut conn, client) = client::new(pair.client().await).await.expect("client init");
        let settings_change = async {
            for _ in 0..10 {
                if client
                    .shared_state()
                    .read("client")
                    .peer_max_field_section_size
                    == 12
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            panic!("peer's max_field_section_size didn't change");
        };

        let drive = async move {
            future::poll_fn(|cx| conn.poll_close(cx)).await.unwrap();
        };

        tokio::select! { _ = settings_change => (), _ = drive => panic!("driver resolved first") };
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::builder()
            .max_field_section_size(12)
            .build(conn)
            .await
            .unwrap();
        incoming.accept().await.unwrap()
    };

    tokio::select! { _ = server_fut => panic!("server resolved first"), _ = client_fut => () };
}

#[tokio::test]
async fn settings_exchange_server() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let (mut conn, _client) = client::builder()
            .max_field_section_size(12)
            .build::<_, _, Bytes>(pair.client().await)
            .await
            .expect("client init");
        let drive = async move {
            future::poll_fn(|cx| conn.poll_close(cx)).await.unwrap();
        };

        tokio::select! { _ = drive => () };
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();

        let state = incoming.shared_state().clone();
        let accept = async { incoming.accept().await.unwrap() };

        let settings_change = async {
            for _ in 0..10 {
                if state.read("setting_change").peer_max_field_section_size == 12 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            panic!("peer's max_field_section_size didn't change");
        };
        tokio::select! { _ = accept => panic!("server resolved first"), _ = settings_change => () };
    };

    tokio::select! { _ = server_fut => (), _ = client_fut => () };
}

#[tokio::test]
async fn client_error_on_bidi_recv() {
    let mut pair = Pair::default();
    let mut server = pair.server();

    macro_rules! check_err {
        ($e:expr) => {
            assert_matches!(
                $e.map(|_| ()).unwrap_err().kind(),
                Kind::Application { reason: Some(reason), code: Code::H3_STREAM_CREATION_ERROR, .. }
                if *reason == *"client received a bidirectional stream");
        }
    }

    let client_fut = async {
        let (mut conn, mut send) = client::new(pair.client().await).await.expect("client init");

        //= https://www.rfc-editor.org/rfc/rfc9114#section-6.1
        //= type=test
        //# Clients MUST treat
        //# receipt of a server-initiated bidirectional stream as a connection
        //# error of type H3_STREAM_CREATION_ERROR unless such an extension has
        //# been negotiated.
        let driver = future::poll_fn(|cx| conn.poll_close(cx));
        check_err!(driver.await);
        check_err!(
            send.send_request(Request::get("http://no.way").body(()).unwrap())
                .await
        );
    };

    let server_fut = async {
        let quinn::NewConnection { connection, .. } =
            server.incoming.next().await.unwrap().await.unwrap();
        let (mut send, _recv) = connection.open_bi().await.unwrap();
        for _ in 0..100 {
            match send.write(b"I'm not really a server").await {
                Err(quinn::WriteError::ConnectionLost(
                    quinn::ConnectionError::ApplicationClosed(quinn::ApplicationClose {
                        error_code,
                        ..
                    }),
                )) if Code::H3_STREAM_CREATION_ERROR == error_code.into_inner() => break,
                Err(e) => panic!("got err: {}", e),
                Ok(_) => tokio::time::sleep(Duration::from_millis(1)).await,
            }
        }
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn two_control_streams() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let new_connection = pair.client_inner().await;

        //= https://www.rfc-editor.org/rfc/rfc9114#section-6.2.1
        //= type=test
        //# Only one control stream per peer is permitted;
        //# receipt of a second stream claiming to be a control stream MUST be
        //# treated as a connection error of type H3_STREAM_CREATION_ERROR.
        for _ in 0..=1 {
            let mut control_stream = new_connection.connection.open_uni().await.unwrap();
            let mut buf = BytesMut::new();
            StreamType::CONTROL.encode(&mut buf);
            control_stream.write_all(&buf[..]).await.unwrap();
        }

        tokio::time::sleep(Duration::from_secs(10)).await;
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        assert_matches!(
            incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Application {
                code: Code::H3_STREAM_CREATION_ERROR,
                ..
            }
        );
    };

    tokio::select! { _ = server_fut => (), _ = client_fut => panic!("client resolved first") };
}

#[tokio::test]
async fn control_close_send_error() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let new_connection = pair.client_inner().await;
        let mut control_stream = new_connection.connection.open_uni().await.unwrap();

        let mut buf = BytesMut::new();
        StreamType::CONTROL.encode(&mut buf);
        control_stream.write_all(&buf[..]).await.unwrap();

        //= https://www.rfc-editor.org/rfc/rfc9114#section-6.2.1
        //= type=test
        //# If either control
        //# stream is closed at any point, this MUST be treated as a connection
        //# error of type H3_CLOSED_CRITICAL_STREAM.
        control_stream.finish().await.unwrap(); // close the client control stream immediately

        let (mut driver, _send) = client::new(h3_quinn::Connection::new(new_connection))
            .await
            .unwrap();

        future::poll_fn(|cx| driver.poll_close(cx)).await
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        // Driver detects that the recieving side of the control stream has been closed
        assert_matches!(
        incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Application { reason: Some(reason), code: Code::H3_CLOSED_CRITICAL_STREAM, .. }
            if *reason == *"control stream closed");
        // Poll it once again returns the previously stored error
        assert_matches!(
            incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Application { reason: Some(reason), code: Code::H3_CLOSED_CRITICAL_STREAM, .. }
            if *reason == *"control stream closed");
    };

    tokio::select! { _ = server_fut => (), _ = client_fut => panic!("client resolved first") };
}

#[tokio::test]
async fn missing_settings() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let new_connection = pair.client_inner().await;
        let mut control_stream = new_connection.connection.open_uni().await.unwrap();

        let mut buf = BytesMut::new();
        StreamType::CONTROL.encode(&mut buf);

        //= https://www.rfc-editor.org/rfc/rfc9114#section-6.2.1
        //= type=test
        //# If the first frame of the control stream is any other frame
        //# type, this MUST be treated as a connection error of type
        //# H3_MISSING_SETTINGS.
        Frame::<Bytes>::CancelPush(StreamId(0)).encode(&mut buf);
        control_stream.write_all(&buf[..]).await.unwrap();

        tokio::time::sleep(Duration::from_secs(10)).await;
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        assert_matches!(
            incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Application {
                code: Code::H3_MISSING_SETTINGS,
                ..
            }
        );
    };

    tokio::select! { _ = server_fut => (), _ = client_fut => panic!("client resolved first") };
}

#[tokio::test]
async fn control_stream_frame_unexpected() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let new_connection = pair.client_inner().await;
        let mut control_stream = new_connection.connection.open_uni().await.unwrap();

        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.1
        //= type=test
        //# If
        //# a DATA frame is received on a control stream, the recipient MUST
        //# respond with a connection error of type H3_FRAME_UNEXPECTED.
        let mut buf = BytesMut::new();
        StreamType::CONTROL.encode(&mut buf);
        Frame::Data(Bytes::from("")).encode(&mut buf);
        control_stream.write_all(&buf[..]).await.unwrap();

        tokio::time::sleep(Duration::from_secs(10)).await;
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        assert_matches!(
            incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Application {
                code: Code::H3_FRAME_UNEXPECTED,
                ..
            }
        );
    };

    tokio::select! { _ = server_fut => (), _ = client_fut => panic!("client resolved first") };
}

#[tokio::test]
async fn timeout_on_control_frame_read() {
    init_tracing();
    let mut pair = Pair::default();
    pair.with_timeout(Duration::from_millis(10));

    let mut server = pair.server();

    let client_fut = async {
        let (mut driver, _send_request) = client::new(pair.client().await).await.unwrap();
        let _ = future::poll_fn(|cx| driver.poll_close(cx)).await;
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        assert_matches!(
            incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Timeout
        );
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn goaway_from_client_not_push_id() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let new_connection = pair.client_inner().await;
        let mut control_stream = new_connection.connection.open_uni().await.unwrap();

        let mut buf = BytesMut::new();
        StreamType::CONTROL.encode(&mut buf);
        Frame::<Bytes>::Settings(Settings::default()).encode(&mut buf);

        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.6
        //= type=test
        //# A client MUST treat receipt of a GOAWAY frame containing a stream ID
        //# of any other type as a connection error of type H3_ID_ERROR.

        // StreamId(index=0 << 2 | dir=Bi << 1 | initiator=Server as u64)
        Frame::<Bytes>::Goaway(StreamId(0u64 << 2 | 0 << 1 | 1)).encode(&mut buf);
        control_stream.write_all(&buf[..]).await.unwrap();

        tokio::time::sleep(Duration::from_secs(10)).await;
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        assert_matches!(
            incoming.accept().await.map(|_| ()).unwrap_err().kind(),
            Kind::Application {
                // The StreamId sent in the GoAway frame from the client is not a PushId:
                code: Code::H3_ID_ERROR,
                ..
            }
        );
    };

    tokio::select! { _ = server_fut => (), _ = client_fut => panic!("client resolved first") };
}

#[tokio::test]
async fn goaway_from_server_not_request_id() {
    init_tracing();
    let mut pair = Pair::default();
    let (_, mut server) = pair.server_inner();

    let client_fut = async {
        let new_connection = pair.client_inner().await;
        let mut control_stream = new_connection.connection.open_uni().await.unwrap();

        let mut buf = BytesMut::new();
        StreamType::CONTROL.encode(&mut buf);
        control_stream.write_all(&buf[..]).await.unwrap();
        control_stream.finish().await.unwrap(); // close the client control stream immediately

        let (mut driver, _send) = client::new(h3_quinn::Connection::new(new_connection))
            .await
            .unwrap();

        assert_matches!(
            future::poll_fn(|cx| driver.poll_close(cx))
                .await
                .unwrap_err()
                .kind(),
            Kind::Application {
                // The sent in the GoAway frame from the client is not a Request:
                code: Code::H3_ID_ERROR,
                ..
            }
        )
    };

    let server_fut = async {
        let conn = server.next().await.unwrap().await.unwrap();
        let mut control_stream = conn.connection.open_uni().await.unwrap();

        let mut buf = BytesMut::new();
        StreamType::CONTROL.encode(&mut buf);
        Frame::<Bytes>::Settings(Settings::default()).encode(&mut buf);

        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.6
        //= type=test
        //# A client MUST treat receipt of a GOAWAY frame containing a stream ID
        //# of any other type as a connection error of type H3_ID_ERROR.

        // StreamId(index=0 << 2 | dir=Uni << 1 | initiator=Server as u64)
        Frame::<Bytes>::Goaway(StreamId(0u64 << 2 | 0 << 1 | 1)).encode(&mut buf);
        control_stream.write_all(&buf[..]).await.unwrap();

        tokio::time::sleep(Duration::from_secs(10)).await;
    };

    tokio::select! { _ = server_fut => panic!("client resolved first"), _ = client_fut => () };
}

#[tokio::test]
async fn graceful_shutdown_server_rejects() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let (_driver, mut send_request) = client::new(pair.client().await).await.unwrap();

        let mut first = send_request
            .send_request(Request::get("http://no.way").body(()).unwrap())
            .await
            .unwrap();
        let mut rejected = send_request
            .send_request(Request::get("http://no.way").body(()).unwrap())
            .await
            .unwrap();
        let first = first.recv_response().await;
        let rejected = rejected.recv_response().await;

        assert_matches!(first, Ok(_));
        assert_matches!(
            rejected.unwrap_err().kind(),
            Kind::Application {
                code: Code::H3_REQUEST_REJECTED,
                ..
            }
        );
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        let (_, stream) = incoming.accept().await.unwrap().unwrap();
        response(stream).await;
        incoming.shutdown(0).await.unwrap();
        assert_matches!(incoming.accept().await.map(|x| x.map(|_| ())), Ok(None));
        server.endpoint.wait_idle().await;
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn graceful_shutdown_grace_interval() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let (mut driver, mut send_request) = client::new(pair.client().await).await.unwrap();

        // Sent as the connection is not shutting down
        let mut first = send_request
            .send_request(Request::get("http://no.way").body(()).unwrap())
            .await
            .unwrap();
        // Sent as the connection is shutting down, but GoAway has not been received yet
        let mut in_flight = send_request
            .send_request(Request::get("http://no.way").body(()).unwrap())
            .await
            .unwrap();
        let first = first.recv_response().await;
        let in_flight = in_flight.recv_response().await;

        // Will not be sent as client's driver already received the GoAway
        let too_late = async move {
            tokio::time::sleep(Duration::from_millis(15)).await;
            request(send_request).await
        };
        let driver = future::poll_fn(|cx| driver.poll_close(cx));

        let (too_late, driver) = tokio::join!(too_late, driver);
        assert_matches!(first, Ok(_));
        assert_matches!(in_flight, Ok(_));
        assert_matches!(too_late.unwrap_err().kind(), Kind::Closing);
        assert_matches!(driver, Ok(_));
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        let (_, first) = incoming.accept().await.unwrap().unwrap();
        incoming.shutdown(1).await.unwrap();
        let (_, in_flight) = incoming.accept().await.unwrap().unwrap();
        response(first).await;
        response(in_flight).await;

        while let Ok(Some((_, stream))) = incoming.accept().await {
            response(stream).await;
        }
        // Ensure `too_late` request is executed as the connection is still
        // closing (no QUIC `Close` frame has been fired yet)
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    tokio::join!(server_fut, client_fut);
}

#[tokio::test]
async fn graceful_shutdown_closes_when_idle() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {
        let (mut driver, mut send_request) = client::new(pair.client().await).await.unwrap();

        // Make continuous requests, ignoring GoAway because the connection is not driven
        while let Ok(_) = request(&mut send_request).await {
            tokio::task::yield_now().await;
        }
        assert_matches!(
            future::poll_fn(|cx| {
                println!("client drive");
                driver.poll_close(cx)
            })
            .await,
            Ok(())
        );
    };

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();

        let mut count = 0;

        while let Ok(Some((_, stream))) = incoming.accept().await {
            if count < 3 {
                count += 1;
            } else if count == 3 {
                count += 1;
                incoming.shutdown(2).await.unwrap();
            }

            response(stream).await;
        }
    };

    tokio::select! {
        _ = client_fut => (),
        r = tokio::time::timeout(Duration::from_millis(100), server_fut)
            => assert_matches!(r, Ok(())),
    };
}

async fn request<T, O, B>(mut send_request: T) -> Result<Response<()>, Error>
where
    T: BorrowMut<SendRequest<O, B>>,
    O: quic::OpenStreams<B>,
    B: Buf,
{
    let mut request_stream = send_request
        .borrow_mut()
        .send_request(Request::get("http://no.way").body(()).unwrap())
        .await?;
    request_stream.recv_response().await
}

async fn response<S, B>(mut stream: server::RequestStream<S, B>)
where
    S: quic::RecvStream + SendStream<B>,
    B: Buf,
{
    stream
        .send_response(
            Response::builder()
                .status(StatusCode::IM_A_TEAPOT)
                .body(())
                .unwrap(),
        )
        .await
        .unwrap();
    stream.finish().await.unwrap();
}
