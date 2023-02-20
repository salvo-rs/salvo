//! The extra lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "basic-auth"]
    pub mod basic_auth;
}

cfg_feature! {
    #![feature = "affix"]
    pub mod affix;
}

cfg_feature! {
    #![feature = "force-https"]
    pub mod force_https;
}

cfg_feature! {
    #![feature = "jwt-auth"]
    pub mod jwt_auth;
}

cfg_feature! {
    #![feature = "catch-panic"]
    pub mod catch_panic;
}

cfg_feature! {
    #![feature = "compression"]
    pub mod compression;
}
cfg_feature! {
    #![feature = "logging"]
    pub mod logging;
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
    #![feature = "trailing-slash"]
    pub mod trailing_slash;
}
cfg_feature! {
    #![feature = "timeout"]
    pub mod timeout;
}
cfg_feature! {
    #![feature = "caching-headers"]
    pub mod caching_headers;
}
