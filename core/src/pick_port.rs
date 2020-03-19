//copy from https://github.com/Dentosal/portpicker-rs
use rand::prelude::*;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, TcpListener, ToSocketAddrs, UdpSocket};

pub type Port = u16;

// Try to bind to a socket using UDP
fn test_bind_udp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(UdpSocket::bind(addr).ok()?.local_addr().ok()?.port())
}

// Try to bind to a socket using TCP
fn test_bind_tcp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(TcpListener::bind(addr).ok()?.local_addr().ok()?.port())
}

/// Check if a port is free on UDP
pub fn is_free_udp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);

    test_bind_udp(ipv6).is_some() && test_bind_udp(ipv4).is_some()
}

/// Check if a port is free on TCP
pub fn is_free_tcp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);

    test_bind_tcp(ipv6).is_some() && test_bind_tcp(ipv4).is_some()
}

/// Check if a port is free on both TCP and UDP
pub fn is_free(port: Port) -> bool {
    is_free_tcp(port) && is_free_udp(port)
}

/// Asks the OS for a free port
fn ask_free_tcp_port() -> Option<Port> {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0);

    test_bind_tcp(ipv6).or_else(|| test_bind_tcp(ipv4))
}

/// Picks an available port that is available on both TCP and UDP
/// ```rust
/// use portpicker::pick_unused_port;
/// let port: u16 = pick_unused_port().expect("No ports free");
/// ```
pub fn pick_unused_port() -> Option<Port> {
    let mut rng = rand::thread_rng();

    // Try random port first
    for _ in 0..10 {
        let port = rng.gen_range(15000, 25000);
        if is_free(port) {
            return Some(port);
        }
    }

    // Ask the OS for a port
    for _ in 0..10 {
        if let Some(port) = ask_free_tcp_port() {
            // Test that the udp port is free as well
            if is_free_udp(port) {
                return Some(port);
            }
        }
    }

    // Give up
    None
}

#[cfg(test)]
mod tests {
    use super::pick_unused_port;

    #[test]
    fn it_works() {
        assert!(pick_unused_port().is_some());
    }
}
