// Copyright 2017-2019 `multipart-async` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::fmt;

pub use futures::*;
use std::task::Poll;

pub fn ready_ok<R, T, E>(val: T) -> Poll<R>
where
    R: From<Result<T, E>>,
{
    Poll::Ready(Ok(val).into())
}

pub fn show_bytes(bytes: &[u8]) -> impl fmt::Display + '_ {
    display_bytes::HEX_UTF8
        .clone()
        .escape_control(true)
        .min_str_len(1)
        .display_bytes(bytes)
}
