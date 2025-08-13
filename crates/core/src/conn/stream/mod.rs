mod straight;
cfg_feature! {
    #![any(feature = "native-tls", feature = "rustls", feature = "openssl", feature = "acme")]
    mod handshake;
    pub use handshake::HandshakeStream;
}
pub use straight::StraightStream;
