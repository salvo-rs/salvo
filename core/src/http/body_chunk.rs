// Copyright 2017-2019 `multipart-async` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! Client- and server-side abstractions for HTTP `multipart/form-data` requests using asynchronous
//! I/O.
//!
//! Features:
//!
//! * `client` (default): Enable the client-side abstractions for multipart requests. If the
//! `hyper` feature is also set, enables integration with the Hyper HTTP client API.
//!
//! * `server` (default): Enable the server-side abstractions for multipart requests. If the
//! `hyper` feature is also set, enables integration with the Hyper HTTP server API.
// FIXME: hiding irrelevant warnings during prototyping
// #![deny(missing_docs)]

use hyper::body::Bytes;
use std::borrow::Cow;
use std::slice::SliceIndex;

pub trait BodyChunk: Sized {
    /// Split the chunk at `idx`, returning `(self[..idx], self[idx..])`.
    fn split_into(self, idx: usize) -> (Self, Self);

    /// Get the slice representing the data of this chunk.
    fn as_slice(&self) -> &[u8];

    /// Slice the bytes in `self` according to the given range.
    #[inline(always)]
    fn slice<R>(&self, range: R) -> &R::Output
    where
        R: SliceIndex<[u8]>,
    {
        &self.as_slice()[range]
    }

    /// Equivalent to `self.as_slice().len()`
    #[inline(always)]
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Equivalent to `self.as_slice().is_empty()`
    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Equivalent to `self.as_slice().to_owned()`
    ///
    /// Implementors are welcome to override this if they can provide a cheaper conversion.
    #[inline(always)]
    fn into_vec(self) -> Vec<u8> {
        self.as_slice().to_owned()
    }
}

impl BodyChunk for Vec<u8> {
    fn split_into(mut self, idx: usize) -> (Self, Self) {
        let other = self.split_off(idx);
        (self, other)
    }

    fn as_slice(&self) -> &[u8] {
        self
    }

    fn into_vec(self) -> Vec<u8> {
        self
    }
}

impl<'a> BodyChunk for &'a [u8] {
    fn split_into(self, idx: usize) -> (Self, Self) {
        self.split_at(idx)
    }

    fn as_slice(&self) -> &[u8] {
        self
    }
}

impl<'a> BodyChunk for Cow<'a, [u8]> {
    fn split_into(self, idx: usize) -> (Self, Self) {
        fn cow_tup<'a, T: Into<Cow<'a, [u8]>>>((left, right): (T, T)) -> (Cow<'a, [u8]>, Cow<'a, [u8]>) {
            (left.into(), right.into())
        }

        match self {
            Cow::Borrowed(slice) => cow_tup(slice.split_into(idx)),
            Cow::Owned(vec) => cow_tup(vec.split_into(idx)),
        }
    }

    fn as_slice(&self) -> &[u8] {
        &**self
    }

    fn into_vec(self) -> Vec<u8> {
        self.into_owned()
    }
}

impl BodyChunk for Bytes {
    fn split_into(mut self, idx: usize) -> (Self, Self) {
        let right = self.split_off(idx);
        (self, right)
    }

    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }
}

// impl BodyChunk for hyper::Body {
//     fn split_into(self, idx: usize) -> (Self, Self) {
//         let (left, right) = self.into_bytes().split_into(idx);
//         (left.into(), right.into())
//     }

//     fn as_slice(&self) -> &[u8] {
//         self
//     }

//     fn into_vec(self) -> Vec<u8> {
//         self.to_vec()
//     }
// }
