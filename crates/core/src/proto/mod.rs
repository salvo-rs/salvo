//! Http protocol supports.
//!
cfg_feature! {
    #![feature = "quinn"]

    pub use salvo_http3::{quic, webtransport};
    pub use salvo_http3::webtransport::server::WebTransportSession;
}
