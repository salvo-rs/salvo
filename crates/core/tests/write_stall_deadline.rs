//! A ready transport cannot revive a write whose stall deadline has passed.

use std::io::{ErrorKind, IoSlice};
use std::task::Poll;
use std::time::Duration;

use salvo_core::conn::{ConnCtrl, StraightStream};
use salvo_core::fuse::FuseConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn late_write(vectored: bool, relaxed: bool) -> std::io::Result<usize> {
    let (client, mut peer) = tokio::io::duplex(1);
    let config = FuseConfig::disabled().with_write_stall_timeout(Duration::from_millis(10));
    let control = ConnCtrl::new();
    let mut stream = StraightStream::new(client, Some(config), control.clone(), None);
    stream.write_all(b"a").await.unwrap();

    let buffers = [IoSlice::new(b"b")];
    let write = async {
        if vectored {
            stream.write_vectored(&buffers).await
        } else {
            stream.write(b"b").await
        }
    };
    tokio::pin!(write);
    let first = std::future::poll_fn(|context| Poll::Ready(write.as_mut().poll(context))).await;
    assert!(first.is_pending());

    // Polling is delayed until both the write and its timer are ready, as can
    // happen when the executor is busy. Keep the idle timeout disabled so only
    // the independent write-stall deadline governs this write.
    tokio::time::advance(Duration::from_millis(40)).await;
    assert_eq!(peer.read_u8().await.unwrap(), b'a');
    if relaxed {
        control.relax_timeouts();
    }
    write.await
}

#[tokio::test(start_paused = true)]
async fn ready_scalar_write_still_obeys_its_stall_deadline() {
    assert_eq!(
        late_write(false, false).await.unwrap_err().kind(),
        ErrorKind::TimedOut
    );
}

#[tokio::test(start_paused = true)]
async fn ready_vectored_write_still_obeys_its_stall_deadline() {
    assert_eq!(
        late_write(true, false).await.unwrap_err().kind(),
        ErrorKind::TimedOut
    );
}

#[tokio::test(start_paused = true)]
async fn relaxed_connections_may_complete_late_writes() {
    assert_eq!(late_write(false, true).await.unwrap(), 1);
    assert_eq!(late_write(true, true).await.unwrap(), 1);
}

#[tokio::test(start_paused = true)]
async fn timely_progress_clears_the_previous_stall_deadline() {
    for vectored in [false, true] {
        let (client, mut peer) = tokio::io::duplex(1);
        let config = FuseConfig::disabled().with_write_stall_timeout(Duration::from_millis(10));
        let mut stream = StraightStream::new(client, Some(config), ConnCtrl::new(), None);
        stream.write_all(b"a").await.unwrap();
        {
            let buffers = [IoSlice::new(b"b")];
            let write = async {
                if vectored {
                    stream.write_vectored(&buffers).await
                } else {
                    stream.write(b"b").await
                }
            };
            tokio::pin!(write);
            let first = std::future::poll_fn(|cx| Poll::Ready(write.as_mut().poll(cx))).await;
            assert!(first.is_pending());
            tokio::time::advance(Duration::from_millis(5)).await;
            assert_eq!(peer.read_u8().await.unwrap(), b'a');
            assert_eq!(write.await.unwrap(), 1);
        }
        tokio::time::advance(Duration::from_millis(20)).await;
        assert_eq!(peer.read_u8().await.unwrap(), b'b');
        assert_eq!(stream.write(b"c").await.unwrap(), 1);
    }
}
