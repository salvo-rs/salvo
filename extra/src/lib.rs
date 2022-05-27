//! The extra lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/images/favicon-32x32.webp")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub, unused_crate_dependencies)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "basic-auth"]
    pub mod basic_auth;
}
cfg_feature! {
    #![feature = "jwt-auth"]
    pub mod jwt_auth;
}

cfg_feature! {
    #![feature = "compression"]
    pub mod compression;
}
cfg_feature! {
    #![feature = "cors"]
    pub mod cors;
}
cfg_feature! {
    #![feature = "csrf"]
    pub mod csrf;
}
cfg_feature! {
    #![feature = "logging"]
    pub mod logging;
}
cfg_feature! {
    #![feature = "proxy"]
    pub mod proxy;
}
cfg_feature! {
    #![feature = "serve-static"]
    pub mod serve_static;
}
cfg_feature! {
    #![feature = "session"]
    pub mod session;
}
cfg_feature! {
    #![feature = "sse"]
    pub mod sse;
}
cfg_feature! {
    #![feature = "ws"]
    pub mod ws;
}

cfg_feature! {
    #![feature =  "size-limiter"]
    pub mod size_limiter;
}
cfg_feature! {
    #![feature = "timeout"]
    pub mod timeout;
}
