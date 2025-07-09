use std::path::PathBuf;
use std::time::Duration;

use crate::{Client, HyperRequest, HyperResponse, Proxy, Upstreams};

use hyper::client::conn::http1::handshake;
use hyper::upgrade::OnUpgrade;
use salvo_core::http::{ReqBody, ResBody};
use salvo_core::rt::tokio::TokioIo;
use salvo_core::{BoxedError, Error};
use tokio::net::UnixStream;
use tokio::time::timeout;

const UNIX_SOCKET_CONNECT_TIMEOUT: u64 = 5; // seconds
/// A client that creates a direct bidirectional channel (TCP tunnel) to a Unix socket.
///
/// This client is designed for scenarios where a raw data stream is established
/// between the client and the upstream service via a Unix socket. It works by
/// "hijacking" the client connection and forwarding all data at the transport layer.
#[derive(Default, Clone, Debug)]
pub struct UnixSockClient;

impl<U> Proxy<U, UnixSockClient>
where
    U: Upstreams,
    U::Error: Into<BoxedError>,
{
    /// Create a new `Proxy` that tunnels connections to a Unix socket.
    pub fn use_unix_sock_tunnel(upstreams: U) -> Self {
        Proxy::new(upstreams, UnixSockClient)
    }
}

impl Client for UnixSockClient {
    type Error = Error;

    async fn execute(
        &self,
        proxied_request: HyperRequest,
        _request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse, Self::Error> {
        let (unix_sock_path, request_path) = extract_unix_paths(proxied_request.uri())?;
        let stream = timeout(
            Duration::from_secs(UNIX_SOCKET_CONNECT_TIMEOUT),
            UnixStream::connect(unix_sock_path),
        )
        .await
        .map_err(|_| Error::other("Connection to unix socket timed out"))?
        .map_err(|e| Error::other(format!("Failed to connect to unix socket: {e}")))?;
        let io = TokioIo::new(stream);
        let (mut sender, conn) = handshake::<_, ReqBody>(io).await.map_err(Error::other)?;
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                tracing::error!(error = ?err, "Connection failed");
            }
        });
        let (mut parts, body) = proxied_request.into_parts();
        parts.uri = request_path.parse().map_err(Error::other)?;
        let final_request = HyperRequest::from_parts(parts, body);
        let response_future = sender.send_request(final_request);
        let response = timeout(Duration::from_secs(30), response_future)
            .await
            .map_err(|_| Error::other("Request to unix socket timed out"))?
            .map_err(Error::other)?
            .map(ResBody::from);
        Ok(response)
    }
}

fn extract_unix_paths(uri: &hyper::Uri) -> Result<(String, String), Error> {
    let full_path = uri.path();
    // assume the pach contains a unix socket path ending with ".sock"
    if let Some(sock_end_index) = full_path.find(".sock") {
        let sock_path_end = sock_end_index + ".sock".len();
        let sock_path_str = &full_path[..sock_path_end];
        let sock_path = PathBuf::from(sock_path_str);
        if sock_path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        {
            return Err(Error::other(
                "Invalid socket path: directory traversal ('..') is not allowed.",
            ));
        }
        let mut request_path = full_path[sock_path_end..].to_string();
        if request_path.is_empty() {
            request_path = "/".to_string();
        }
        if let Some(query) = uri.query() {
            request_path.push('?');
            request_path.push_str(query);
        }
        Ok((sock_path_str.to_string(), request_path))
    } else {
        Err(Error::other(
            "Could not find a .sock file in the URI path to determine the unix socket path.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_creation() {
        let upstreams = vec!["http://unix:/var/run/my.sock"];
        let proxy = Proxy::new(upstreams.clone(), UnixSockClient);
        assert_eq!(proxy.upstreams().len(), 1);
    }
}
