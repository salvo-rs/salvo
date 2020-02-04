use std::fmt;
// use std::mem;
use display_bytes;

pub use futures::*;
use std::task::Poll;

pub fn ready_ok<R, T, E>(val: T) -> Poll<R>
where
    R: From<Result<T, E>>,
{
    Poll::Ready(Ok(val).into())
}
// pub fn replace_default<T: Default>(dest: &mut T) -> T {
//     mem::replace(dest, T::default())
// }

pub fn show_bytes(bytes: &[u8]) -> impl fmt::Display + '_ {
    display_bytes::HEX_UTF8
        .clone()
        .escape_control(true)
        .min_str_len(1)
        .display_bytes(bytes)
}
