use bytes::{Buf, BufMut};

use super::varint::VarInt;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct UnexpectedEnd(pub usize);

pub type Result<T> = ::std::result::Result<T, UnexpectedEnd>;

// Trait for encoding / decoding helpers on basic types, such as `u16`, for
// example: `buf.decode::<u16>()?`.
// This enables to return `UnexpectedEnd` instead of panicking as the `Buf`
// impls do when there is not enough bytes.

pub trait Encode {
    fn encode<B: BufMut>(&self, buf: &mut B);
}

pub trait Decode: Sized {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self>;
}

impl Encode for u8 {
    fn encode<B: BufMut>(&self, buf: &mut B) {
        buf.put_u8(*self);
    }
}

impl Decode for u8 {
    fn decode<B: Buf>(buf: &mut B) -> Result<u8> {
        if buf.remaining() < 1 {
            return Err(UnexpectedEnd(1));
        }
        Ok(buf.get_u8())
    }
}

pub trait BufExt {
    fn get<T: Decode>(&mut self) -> Result<T>;
    fn get_var(&mut self) -> Result<u64>;
}

impl<T: Buf> BufExt for T {
    fn get<U: Decode>(&mut self) -> Result<U> {
        U::decode(self)
    }

    fn get_var(&mut self) -> Result<u64> {
        Ok(VarInt::decode(self)?.into_inner())
    }
}

pub trait BufMutExt {
    fn write<T: Encode>(&mut self, x: T);
    fn write_var(&mut self, x: u64);
}

impl<T: BufMut> BufMutExt for T {
    fn write<U: Encode>(&mut self, x: U) {
        x.encode(self);
    }

    fn write_var(&mut self, x: u64) {
        VarInt::from_u64(x).unwrap().encode(self);
    }
}
