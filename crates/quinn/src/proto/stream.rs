use bytes::{Buf, BufMut};
use std::{
    convert::TryFrom,
    fmt::{self, Display},
    ops::Add,
};

use super::{
    coding::{BufExt, BufMutExt, Decode, Encode, UnexpectedEnd},
    varint::VarInt,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StreamType(u64);

macro_rules! stream_types {
    {$($name:ident = $val:expr,)*} => {
        impl StreamType {
            $(pub const $name: StreamType = StreamType($val);)*
        }
    }
}

stream_types! {
    CONTROL = 0x00,
    PUSH = 0x01,
    ENCODER = 0x02,
    DECODER = 0x03,
}

impl StreamType {
    pub const MAX_ENCODED_SIZE: usize = VarInt::MAX_SIZE;

    pub fn value(&self) -> u64 {
        self.0
    }
    /// returns a StreamType type with random number of the 0x1f * N + 0x21
    /// format within the range of the Varint implementation
    pub fn grease() -> Self {
        StreamType(fastrand::u64(0..0x210842108421083) * 0x1f + 0x21)
    }
}

impl Decode for StreamType {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, UnexpectedEnd> {
        Ok(StreamType(buf.get_var()?))
    }
}

impl Encode for StreamType {
    fn encode<W: BufMut>(&self, buf: &mut W) {
        buf.write_var(self.0);
    }
}

impl fmt::Display for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            &StreamType::CONTROL => write!(f, "Control"),
            &StreamType::ENCODER => write!(f, "Encoder"),
            &StreamType::DECODER => write!(f, "Decoder"),
            x => write!(f, "StreamType({})", x.0),
        }
    }
}

/// Identifier for a stream
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct StreamId(#[cfg(not(test))] u64, #[cfg(test)] pub(crate) u64);

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let initiator = match self.initiator() {
            Side::Client => "client",
            Side::Server => "server",
        };
        let dir = match self.dir() {
            Dir::Uni => "uni",
            Dir::Bi => "bi",
        };
        write!(
            f,
            "{} {}directional stream {}",
            initiator,
            dir,
            self.index()
        )
    }
}

impl StreamId {
    pub(crate) fn first_request() -> Self {
        Self::new(0, Dir::Bi, Side::Client)
    }

    /// Is this a client-initiated request?
    pub fn is_request(&self) -> bool {
        self.dir() == Dir::Bi && self.initiator() == Side::Client
    }

    /// Is this a server push?
    pub fn is_push(&self) -> bool {
        self.dir() == Dir::Uni && self.initiator() == Side::Server
    }

    /// Which side of a connection initiated the stream
    pub(crate) fn initiator(self) -> Side {
        if self.0 & 0x1 == 0 {
            Side::Client
        } else {
            Side::Server
        }
    }

    /// Create a new StreamId
    fn new(index: u64, dir: Dir, initiator: Side) -> Self {
        StreamId((index as u64) << 2 | (dir as u64) << 1 | initiator as u64)
    }

    /// Distinguishes streams of the same initiator and directionality
    fn index(self) -> u64 {
        self.0 >> 2
    }

    /// Which directions data flows in
    fn dir(self) -> Dir {
        if self.0 & 0x2 == 0 {
            Dir::Bi
        } else {
            Dir::Uni
        }
    }
}

impl TryFrom<u64> for StreamId {
    type Error = InvalidStreamId;
    fn try_from(v: u64) -> Result<Self, Self::Error> {
        if v > VarInt::MAX.0 {
            return Err(InvalidStreamId(v));
        }
        Ok(Self(v))
    }
}

/// Invalid StreamId, for example because it's too large
#[derive(Debug, PartialEq)]
pub struct InvalidStreamId(u64);

impl Display for InvalidStreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid stream id: {:x}", self.0)
    }
}

impl Encode for StreamId {
    fn encode<B: bytes::BufMut>(&self, buf: &mut B) {
        VarInt::from_u64(self.0).unwrap().encode(buf);
    }
}

impl Add<usize> for StreamId {
    type Output = StreamId;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn add(self, rhs: usize) -> Self::Output {
        let index = u64::min(
            u64::saturating_add(self.index(), rhs as u64),
            VarInt::MAX.0 >> 2,
        );
        Self::new(index, self.dir(), self.initiator())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Side {
    /// The initiator of a connection
    Client = 0,
    /// The acceptor of a connection
    Server = 1,
}

/// Whether a stream communicates data in both directions or only from the initiator
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Dir {
    /// Data flows in both directions
    Bi = 0,
    /// Data flows only from the stream's initiator
    Uni = 1,
}
