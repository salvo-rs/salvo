//! addr module
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

/// Network socket address
#[derive(Debug, Clone)]
pub enum SocketAddr {
    /// IPv4 socket address
    IPv4(std::net::SocketAddrV4),
    /// IPv6 socket address
    IPv6(std::net::SocketAddrV6),
    /// Unix socket address
    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    Unix(Arc<tokio::net::unix::SocketAddr>),
}
impl From<std::net::SocketAddr> for SocketAddr {
    fn from(addr: std::net::SocketAddr) -> Self {
        match addr {
            std::net::SocketAddr::V4(val) => SocketAddr::IPv4(val),
            std::net::SocketAddr::V6(val) => SocketAddr::IPv6(val),
        }
    }
}
impl From<std::net::SocketAddrV4> for SocketAddr {
    fn from(addr: std::net::SocketAddrV4) -> Self {
        SocketAddr::IPv4(addr)
    }
}
impl From<std::net::SocketAddrV6> for SocketAddr {
    fn from(addr: std::net::SocketAddrV6) -> Self {
        SocketAddr::IPv6(addr)
    }
}

#[cfg(unix)]
impl From<tokio::net::unix::SocketAddr> for SocketAddr {
    fn from(addr: tokio::net::unix::SocketAddr) -> Self {
        SocketAddr::Unix(addr.into())
    }
}
impl SocketAddr {
    /// Returns is a ipv4 socket address.
    pub fn is_ipv4(&self) -> bool {
        matches!(*self, SocketAddr::IPv4(_))
    }
    /// Returns is a ipv6 socket address.
    pub fn is_ipv6(&self) -> bool {
        matches!(*self, SocketAddr::IPv6(_))
    }

    /// Returns is a unix socket address.
    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    pub fn is_unix(&self) -> bool {
        matches!(*self, SocketAddr::Unix(_))
    }

    /// Returns ipv6 socket address.
    pub fn as_ipv6(&self) -> Option<&std::net::SocketAddrV6> {
        match self {
            SocketAddr::IPv6(addr) => Some(addr),
            _ => None,
        }
    }
    /// Returns ipv4 socket address.
    pub fn as_ipv4(&self) -> Option<&std::net::SocketAddrV4> {
        match self {
            SocketAddr::IPv4(addr) => Some(addr),
            _ => None,
        }
    }

    /// Returns unix socket address.
    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    pub fn as_unix(&self) -> Option<&tokio::net::unix::SocketAddr> {
        match self {
            SocketAddr::Unix(addr) => Some(addr),
            _ => None,
        }
    }
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SocketAddr::IPv4(addr) => write!(f, "socket://{}", addr),
            SocketAddr::IPv6(addr) => write!(f, "socket://{}", addr),
            #[cfg(unix)]
            SocketAddr::Unix(addr) => match addr.as_pathname() {
                Some(path) => write!(f, "unix://{}", path.display()),
                None => f.write_str("unix://unknown"),
            },
        }
    }
}
