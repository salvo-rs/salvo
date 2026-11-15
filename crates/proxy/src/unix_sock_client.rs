use std::path::PathBuf;
use std::time::Duration;

use crate::{Client, HyperRequest, HyperResponse, Proxy, Upstreams};

use hyper::client::conn::http1::handshake;
use hyper::upgrade::OnUpgrade;
use salvo_core::http::{ReqBody, ResBody};
use salvo_core::rt::tokio::TokioIo;
use salvo_core::{BoxedError, Error};
use tokio::net::UnixStream;
use tokio::io::copy_bidirectional;
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
        Self::new(upstreams, UnixSockClient)
    }
}

impl Client for UnixSockClient {
    type Error = Error;

    async fn execute(
        &self,
        proxied_request: HyperRequest,
        request_upgraded: Option<OnUpgrade>,
    ) -> Result<HyperResponse, Self::Error> {
        let (unix_sock_path, request_path) = extract_unix_paths(proxied_request.uri())?;
        execute_unix(proxied_request, request_upgraded, unix_sock_path.into(), request_path).await
    }
}

/// A Unix proxy client with an explicit socket path, independent of HTTP paths.
#[derive(Clone, Debug)]
pub struct ConfiguredUnixSockClient {
    socket_path: PathBuf,
}

impl UnixSockClient {
    /// Select a socket explicitly, including paths with arbitrary directory names.
    /// The upstream URI's path and query are then used only as the HTTP target.
    pub fn with_socket_path(path: impl Into<PathBuf>) -> ConfiguredUnixSockClient {
        ConfiguredUnixSockClient { socket_path: path.into() }
    }
}

impl Client for ConfiguredUnixSockClient {
    type Error = Error;
    async fn execute(&self, request: HyperRequest, upgraded: Option<OnUpgrade>) -> Result<HyperResponse, Self::Error> {
        let target = request.uri().path_and_query().map(|v| v.as_str()).unwrap_or("/").to_owned();
        execute_unix(request, upgraded, self.socket_path.clone(), target).await
    }
}

async fn execute_unix(
    proxied_request: HyperRequest,
    request_upgraded: Option<OnUpgrade>,
    socket_path: PathBuf,
    request_path: String,
) -> Result<HyperResponse, Error> {
        let request_upgrade = crate::get_upgrade_type(proxied_request.headers());
        let conn_ctrl = proxied_request.extensions().get::<salvo_core::conn::ConnCtrl>().cloned();
        let stream = timeout(
            Duration::from_secs(UNIX_SOCKET_CONNECT_TIMEOUT),
            UnixStream::connect(socket_path),
        )
        .await
        .map_err(|_| Error::other("Connection to unix socket timed out"))?
        .map_err(|e| Error::other(format!("Failed to connect to unix socket: {e}")))?;
        let io = TokioIo::new(stream);
        let (mut sender, conn) = handshake::<_, ReqBody>(io).await.map_err(Error::other)?;
        tokio::spawn(async move {
            if let Err(err) = conn.with_upgrades().await {
                tracing::error!(error = ?err, "Connection failed");
            }
        });
        let (mut parts, body) = proxied_request.into_parts();
        parts.uri = request_path.parse().map_err(Error::other)?;
        let final_request = HyperRequest::from_parts(parts, body);
        let response_future = sender.send_request(final_request);
        let mut response = timeout(Duration::from_secs(30), response_future)
            .await
            .map_err(|_| Error::other("Request to unix socket timed out"))?
            .map_err(Error::other)?;
        if response.status() == salvo_core::http::StatusCode::SWITCHING_PROTOCOLS {
            let selected = crate::get_upgrade_type(response.headers());
            if !crate::upgrade_types_match(request_upgrade.as_deref(), selected.as_deref()) {
                return Err(Error::other("upgrade type mismatch"));
            }
            let request_upgraded = request_upgraded.ok_or_else(|| Error::other("request does not have an upgrade extension"))?;
            let response_upgraded = hyper::upgrade::on(&mut response).await?;
            crate::spawn_upgrade(conn_ctrl, async move {
                match request_upgraded.await {
                    Ok(request_upgraded) => {
                        let mut incoming = TokioIo::new(request_upgraded);
                        let mut outgoing = TokioIo::new(response_upgraded);
                        if let Err(error) = copy_bidirectional(&mut incoming, &mut outgoing).await {
                            tracing::error!(?error, "Unix socket upgrade transfer failed");
                        }
                    }
                    Err(error) => tracing::debug!(?error, "Unix socket request upgrade failed"),
                }
            });
        }
        Ok(response.map(ResBody::from))
}

fn extract_unix_paths(uri: &hyper::Uri) -> Result<(String, String), Error> {
    let full_path = uri.path();
    // Assume the path contains a unix socket path ending with ".sock"
    if let Some((sock_end_index, _)) = full_path.match_indices(".sock").find(|(index, _)| {
        let remainder = &full_path[index + ".sock".len()..];
        remainder.is_empty() || remainder.starts_with('/')
    }) {
        let sock_path_end = sock_end_index + ".sock".len();
        let sock_path_str = full_path.get(..sock_path_end).ok_or_else(|| {
            Error::other("Invalid URI path: socket path boundary is not valid UTF-8.")
        })?;
        let sock_path = PathBuf::from(sock_path_str);
        if sock_path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        {
            return Err(Error::other(
                "Invalid socket path: directory traversal ('..') is not allowed.",
            ));
        }
        let mut request_path = full_path
            .get(sock_path_end..)
            .ok_or_else(|| {
                Error::other("Invalid URI path: request path boundary is not valid UTF-8.")
            })?
            .to_owned();
        if request_path.is_empty() {
            request_path = "/".to_owned();
        }
        if let Some(query) = uri.query() {
            request_path.push('?');
            request_path.push_str(query);
        }
        Ok((sock_path_str.to_owned(), request_path))
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
    fn socket_suffix_is_a_complete_path_component() {
        let uri = "http://unix/run/app.sock.d/server.sock/api?q=1".parse().unwrap();
        assert_eq!(extract_unix_paths(&uri).unwrap(), ("/run/app.sock.d/server.sock".into(), "/api?q=1".into()));
    }

    #[test]
    fn test_proxy_creation() {
        let upstreams = vec!["http://unix:/var/run/my.sock"];
        let proxy = Proxy::new(upstreams.clone(), UnixSockClient);
        assert_eq!(proxy.upstreams().len(), 1);
    }
}
