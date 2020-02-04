// Copyright 2017-2019 `multipart-async` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! ### Note: not stable APIS
//! The items exported in this module are not considered part of this crate's public API
//! and may receive breaking changes in semver-compatible versions.
use std::future::Future;
use std::task::{Context, Poll};
use std::thread;

use futures::stream::{Stream, TryStream};

use futures_test::stream::StreamTestExt;
use futures_test::task::noop_context;

use futures_util::stream::{self, StreamExt};
use std::convert::Infallible;

use crate::http::errors::ReadError;

pub const BOUNDARY: &str = "--boundary";

pub const TEST_SINGLE_FIELD: &[&[u8]] = &[
    b"--boundary\r",
    b"\n",
    b"Content-Disposition:",
    b" form-data; name=",
    b"\"foo\"",
    b"\r\n\r\n",
    b"field data",
    b"\r",
    b"\n--boundary--",
];

pub fn mock_stream<'d>(
    test_data: &'d [&'d [u8]],
) -> impl Stream<Item = Result<&'d [u8], ReadError>> + 'd {
    stream::iter(test_data.iter().cloned())
        .map(Ok)
        .interleave_pending()
}

macro_rules! until_ready(
    (|$cx:ident| $expr:expr) => {{
        use std::task::Poll::*;
        let ref mut $cx = futures_test::task::noop_context();
        loop {
            match $expr {
                Ready(val) => break val,
                Pending => (),
            }
        }
    }}
);

macro_rules! ready_assert_eq(
    (|$cx:ident| $expr:expr, $eq:expr) => {{
        use std::task::Poll::*;
        let ref mut $cx = futures_test::task::noop_context();
        loop {
            match $expr {
                Ready(val) => {
                    assert_eq!(val, $eq);
                    break;
                },
                Pending => (),
            }
        }
    }}
);
macro_rules! ready_assert_eq_none(
    (|$cx:ident| $expr:expr) => {{
        use std::task::Poll::*;
        let ref mut $cx = futures_test::task::noop_context();
        loop {
            match $expr {
                Ready(val) => {
                    assert_eq!(val.is_none(), true);
                    break;
                },
                Pending => (),
            }
        }
    }}
);

macro_rules! ready_assert_ok_eq(
    (|$cx:ident| $expr:expr, $eq:expr) => {{
        use std::task::Poll::*;
        let ref mut $cx = futures_test::task::noop_context();
        loop {
            match $expr {
                Ready(val) => {
                    assert_eq!(val.unwrap(), $eq);
                    break;
                },
                Pending => (),
            }
        }
    }}
);
macro_rules! ready_assert_some_ok_eq(
    (|$cx:ident| $expr:expr, $eq:expr) => {{
        use std::task::Poll::*;
        let ref mut $cx = futures_test::task::noop_context();
        loop {
            match $expr {
                Ready(val) => {
                    assert_eq!(val.unwrap().unwrap(), $eq);
                    break;
                },
                Pending => (),
            }
        }
    }}
);

macro_rules! ready_assert(
    (|$cx:ident| $expr:expr) => {{
        use std::task::Poll::*;
        let ref mut $cx = futures_test::task::noop_context();
        loop {
            match $expr {
                Ready(val) => {
                    assert!(val);
                    break;
                },
                Pending => (),
            }
        }
    }}
);

pub fn run_future_hot<F>(f: F) -> F::Output
where
    F: Future,
{
    pin_mut!(f);
    until_ready!(|cx| f.as_mut().poll(cx))
}

pub fn assert_unpin<T: Unpin>() {}
