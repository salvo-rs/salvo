use bytes::{Buf, BufMut, Bytes};
use std::{convert::TryInto, fmt};
use tracing::trace;

use super::{
    coding::Encode,
    stream::{InvalidStreamId, StreamId},
    varint::{BufExt, BufMutExt, UnexpectedEnd, VarInt},
};

#[derive(Debug, PartialEq)]
pub enum FrameError {
    Malformed,
    UnsupportedFrame(u64), // Known frames that should generate an error
    UnknownFrame(u64),     // Unknown frames that should be ignored
    InvalidFrameValue,
    Incomplete(usize),
    Settings(SettingsError),
    InvalidStreamId(InvalidStreamId),
}

impl std::error::Error for FrameError {}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::Malformed => write!(f, "frame is malformed"),
            FrameError::UnsupportedFrame(c) => write!(f, "frame 0x{:x} is not allowed h3", c),
            FrameError::UnknownFrame(c) => write!(f, "frame 0x{:x} ignored", c),
            FrameError::InvalidFrameValue => write!(f, "frame value is invalid"),
            FrameError::Incomplete(x) => write!(f, "internal error: frame incomplete {}", x),
            FrameError::Settings(x) => write!(f, "invalid settings: {}", x),
            FrameError::InvalidStreamId(x) => write!(f, "invalid stream id: {}", x),
        }
    }
}

pub enum Frame<B> {
    Data(B),
    Headers(Bytes),
    CancelPush(StreamId),
    Settings(Settings),
    PushPromise(PushPromise),
    Goaway(StreamId),
    MaxPushId(StreamId),
    Grease,
}

/// Represents the available data len for a `Data` frame on a RecvStream
///
/// Decoding received frames does not handle `Data` frames payload. Instead, receiving it
/// and passing it to the user is left under the responsability of `RequestStream`s.
pub struct PayloadLen(pub usize);

impl From<usize> for PayloadLen {
    fn from(len: usize) -> Self {
        PayloadLen(len)
    }
}

impl Frame<PayloadLen> {
    pub const MAX_ENCODED_SIZE: usize = VarInt::MAX_SIZE * 3;

    pub fn decode<T: Buf>(buf: &mut T) -> Result<Self, FrameError> {
        let remaining = buf.remaining();
        let ty = FrameType::decode(buf).map_err(|_| FrameError::Incomplete(remaining + 1))?;
        let len = buf
            .get_var()
            .map_err(|_| FrameError::Incomplete(remaining + 1))?;

        if ty == FrameType::DATA {
            return Ok(Frame::Data((len as usize).into()));
        }

        if buf.remaining() < len as usize {
            return Err(FrameError::Incomplete(2 + len as usize));
        }

        let mut payload = buf.take(len as usize);
        let frame = match ty {
            FrameType::HEADERS => Ok(Frame::Headers(payload.copy_to_bytes(len as usize))),
            FrameType::SETTINGS => Ok(Frame::Settings(Settings::decode(&mut payload)?)),
            FrameType::CANCEL_PUSH => Ok(Frame::CancelPush(payload.get_var()?.try_into()?)),
            FrameType::PUSH_PROMISE => Ok(Frame::PushPromise(PushPromise::decode(&mut payload)?)),
            FrameType::GOAWAY => Ok(Frame::Goaway(payload.get_var()?.try_into()?)),
            FrameType::MAX_PUSH_ID => Ok(Frame::MaxPushId(payload.get_var()?.try_into()?)),
            FrameType::H2_PRIORITY
            | FrameType::H2_PING
            | FrameType::H2_WINDOW_UPDATE
            | FrameType::H2_CONTINUATION => Err(FrameError::UnsupportedFrame(ty.0)),
            _ => {
                buf.advance(len as usize);
                Err(FrameError::UnknownFrame(ty.0))
            }
        };
        if let Ok(frame) = &frame {
            trace!(
                "got frame {:?}, len: {}, remaining: {}",
                frame,
                len,
                buf.remaining()
            );
        }
        frame
    }
}

impl<B> Encode for Frame<B>
where
    B: Buf,
{
    fn encode<T: BufMut>(&self, buf: &mut T) {
        match self {
            Frame::Data(b) => {
                FrameType::DATA.encode(buf);
                buf.write_var(b.remaining() as u64);
            }
            Frame::Headers(f) => {
                FrameType::HEADERS.encode(buf);
                buf.write_var(f.len() as u64);
            }
            Frame::Settings(f) => f.encode(buf),
            Frame::PushPromise(f) => f.encode(buf),
            Frame::CancelPush(id) => simple_frame_encode(FrameType::CANCEL_PUSH, *id, buf),
            Frame::Goaway(id) => simple_frame_encode(FrameType::GOAWAY, *id, buf),
            Frame::MaxPushId(id) => simple_frame_encode(FrameType::MAX_PUSH_ID, *id, buf),
            Frame::Grease => {
                FrameType::grease().encode(buf);
                buf.write_var(6);
                buf.put_slice(b"grease");
            }
        }
    }
}

impl<B> Frame<B>
where
    B: Buf,
{
    pub fn payload(&self) -> Option<&dyn Buf> {
        match self {
            Frame::Data(f) => Some(f),
            Frame::Headers(f) => Some(f),
            Frame::PushPromise(f) => Some(&f.encoded),
            _ => None,
        }
    }

    pub fn payload_mut(&mut self) -> Option<&mut dyn Buf> {
        match self {
            Frame::Data(f) => Some(f),
            Frame::Headers(f) => Some(f),
            Frame::PushPromise(f) => Some(&mut f.encoded),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn encode_with_payload<T: BufMut>(&mut self, buf: &mut T) {
        self.encode(buf);
        match self {
            Frame::Data(b) => {
                while b.has_remaining() {
                    let pos = {
                        let chunk = b.chunk();
                        buf.put_slice(chunk);
                        chunk.len()
                    };
                    b.advance(pos)
                }
            }
            Frame::Headers(b) => buf.put_slice(b),
            _ => (),
        }
    }
}

impl fmt::Debug for Frame<PayloadLen> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Frame::Data(len) => write!(f, "Data: {} bytes", len.0),
            Frame::Headers(frame) => write!(f, "Headers({} entries)", frame.len()),
            Frame::Settings(_) => write!(f, "Settings"),
            Frame::CancelPush(id) => write!(f, "CancelPush({})", id),
            Frame::PushPromise(frame) => write!(f, "PushPromise({})", frame.id),
            Frame::Goaway(id) => write!(f, "GoAway({})", id),
            Frame::MaxPushId(id) => write!(f, "MaxPushId({})", id),
            Frame::Grease => write!(f, "Grease()"),
        }
    }
}

impl<B> fmt::Debug for Frame<B>
where
    B: Buf,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Frame::Data(data) => write!(f, "Data: {} bytes", data.remaining()),
            Frame::Headers(frame) => write!(f, "Headers({} entries)", frame.len()),
            Frame::Settings(_) => write!(f, "Settings"),
            Frame::CancelPush(id) => write!(f, "CancelPush({})", id),
            Frame::PushPromise(frame) => write!(f, "PushPromise({})", frame.id),
            Frame::Goaway(id) => write!(f, "GoAway({})", id),
            Frame::MaxPushId(id) => write!(f, "MaxPushId({})", id),
            Frame::Grease => write!(f, "Grease()"),
        }
    }
}

/// Compare two frames ignoring data
///
/// Only useful for `encode() -> Frame<Buf>` then `decode() -> Frame<PayloadLen>` unit tests.
#[cfg(test)]
impl<T, U> PartialEq<Frame<T>> for Frame<U> {
    fn eq(&self, other: &Frame<T>) -> bool {
        match self {
            Frame::Data(_) => matches!(other, Frame::Data(_)),
            Frame::Settings(x) => matches!(other, Frame::Settings(y) if x == y),
            Frame::Headers(x) => matches!(other, Frame::Headers(y) if x == y),
            Frame::CancelPush(x) => matches!(other, Frame::CancelPush(y) if x == y),
            Frame::PushPromise(x) => matches!(other, Frame::PushPromise(y) if x == y),
            Frame::Goaway(x) => matches!(other, Frame::Goaway(y) if x == y),
            Frame::MaxPushId(x) => matches!(other, Frame::MaxPushId(y) if x == y),
            Frame::Grease => matches!(other, Frame::Grease),
        }
    }
}

#[cfg(test)]
impl Frame<Bytes> {
    pub fn headers<T: Into<Bytes>>(block: T) -> Self {
        Frame::Headers(block.into())
    }
}

macro_rules! frame_types {
    {$($name:ident = $val:expr,)*} => {
        impl FrameType {
            $(pub const $name: FrameType = FrameType($val);)*
        }
    }
}

frame_types! {
    DATA = 0x0,
    HEADERS = 0x1,
    H2_PRIORITY = 0x2,
    CANCEL_PUSH = 0x3,
    SETTINGS = 0x4,
    PUSH_PROMISE = 0x5,
    H2_PING = 0x6,
    GOAWAY = 0x7,
    H2_WINDOW_UPDATE = 0x8,
    H2_CONTINUATION = 0x9,
    MAX_PUSH_ID = 0xD,
}

impl FrameType {
    /// returns a FrameType type with random number of the 0x1f * N + 0x21
    /// format within the range of the Varint implementation
    pub fn grease() -> Self {
        FrameType(fastrand::u64(0..0x210842108421083) * 0x1f + 0x21)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FrameType(u64);

impl FrameType {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, UnexpectedEnd> {
        Ok(FrameType(buf.get_var()?))
    }
    pub fn encode<B: BufMut>(&self, buf: &mut B) {
        buf.write_var(self.0);
    }

    #[cfg(test)]
    pub(crate) const RESERVED: FrameType = FrameType(0x1f * 1337 + 0x21);
}

pub(crate) trait FrameHeader {
    fn len(&self) -> usize;
    const TYPE: FrameType;
    fn encode_header<T: BufMut>(&self, buf: &mut T) {
        Self::TYPE.encode(buf);
        buf.write_var(self.len() as u64);
    }
}

#[derive(Debug, PartialEq)]
pub struct PushPromise {
    id: u64,
    encoded: Bytes,
}

impl FrameHeader for PushPromise {
    const TYPE: FrameType = FrameType::PUSH_PROMISE;

    fn encode_header<T: BufMut>(&self, buf: &mut T) {
        Self::TYPE.encode(buf);
        buf.write_var(self.len() as u64);
        buf.write_var(self.id);
    }

    fn len(&self) -> usize {
        VarInt::from_u64(self.id)
            .expect("PushPromise id varint overflow")
            .size()
            + self.encoded.as_ref().len()
    }
}

impl PushPromise {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, UnexpectedEnd> {
        Ok(PushPromise {
            id: buf.get_var()?,
            encoded: buf.copy_to_bytes(buf.remaining()),
        })
    }
    fn encode<B: BufMut>(&self, buf: &mut B) {
        self.encode_header(buf);
        buf.put(self.encoded.clone());
    }
}

fn simple_frame_encode<B: BufMut>(ty: FrameType, id: StreamId, buf: &mut B) {
    ty.encode(buf);
    buf.write_var(1);
    id.encode(buf);
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct SettingId(pub u64);

impl SettingId {
    const NONE: SettingId = SettingId(0);

    /// returns a SettingId type with random number of the 0x1f * N + 0x21
    /// format within the range of the Varint implementation
    pub fn grease() -> Self {
        SettingId(fastrand::u64(0..0x210842108421083) * 0x1f + 0x21)
    }

    fn is_supported(self) -> bool {
        matches!(
            self,
            SettingId::MAX_HEADER_LIST_SIZE
                | SettingId::QPACK_MAX_TABLE_CAPACITY
                | SettingId::QPACK_MAX_BLOCKED_STREAMS,
        )
    }

    /// Returns if a Settings Identifier is forbidden
    fn is_forbidden(&self) -> bool {
        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4.1
        //# Setting identifiers that were defined in [HTTP/2] where there is no
        //# corresponding HTTP/3 setting have also been reserved
        //# (Section 11.2.2).  These reserved settings MUST NOT be sent, and
        //# their receipt MUST be treated as a connection error of type
        //# H3_SETTINGS_ERROR.
        matches!(
            self,
            SettingId(0x00) | SettingId(0x02) | SettingId(0x03) | SettingId(0x04) | SettingId(0x05)
        )
    }

    fn decode<B: Buf>(buf: &mut B) -> Result<Self, UnexpectedEnd> {
        Ok(SettingId(buf.get_var()?))
    }

    fn encode<B: BufMut>(&self, buf: &mut B) {
        buf.write_var(self.0);
    }
}

macro_rules! setting_identifiers {
    {$($name:ident = $val:expr,)*} => {
        impl SettingId {
            $(pub const $name: SettingId = SettingId($val);)*
        }
    }
}

setting_identifiers! {
    QPACK_MAX_TABLE_CAPACITY = 0x1,
    QPACK_MAX_BLOCKED_STREAMS = 0x7,
    MAX_HEADER_LIST_SIZE = 0x6,
}

const SETTINGS_LEN: usize = 4;

#[derive(Debug, PartialEq)]
pub struct Settings {
    entries: [(SettingId, u64); SETTINGS_LEN],
    len: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            entries: [(SettingId::NONE, 0); SETTINGS_LEN],
            len: 0,
        }
    }
}

impl FrameHeader for Settings {
    const TYPE: FrameType = FrameType::SETTINGS;
    fn len(&self) -> usize {
        self.entries[..self.len].iter().fold(0, |len, (id, val)| {
            len + VarInt::from_u64(id.0).unwrap().size() + VarInt::from_u64(*val).unwrap().size()
        })
    }
}

impl Settings {
    pub const MAX_ENCODED_SIZE: usize = SETTINGS_LEN * 2 * VarInt::MAX_SIZE;

    pub fn insert(&mut self, id: SettingId, value: u64) -> Result<(), SettingsError> {
        if self.len >= self.entries.len() {
            return Err(SettingsError::Exceeded);
        }

        //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4
        //# The same setting identifier MUST NOT occur more than once in the
        //# SETTINGS frame.
        if self.entries[..self.len].iter().any(|(i, _)| *i == id) {
            return Err(SettingsError::Repeated(id));
        }

        self.entries[self.len] = (id, value);
        self.len += 1;
        Ok(())
    }

    pub fn get(&self, id: SettingId) -> Option<u64> {
        for (entry_id, value) in self.entries.iter() {
            if id == *entry_id {
                return Some(*value);
            }
        }
        None
    }

    pub(super) fn encode<T: BufMut>(&self, buf: &mut T) {
        self.encode_header(buf);
        for (id, val) in self.entries[..self.len].iter() {
            id.encode(buf);
            buf.write_var(*val);
        }
    }

    pub(super) fn decode<T: Buf>(buf: &mut T) -> Result<Settings, SettingsError> {
        let mut settings = Settings::default();
        while buf.has_remaining() {
            if buf.remaining() < 2 {
                // remains less than 2 * minimum-size varint
                return Err(SettingsError::Malformed);
            }

            let identifier = SettingId::decode(buf).map_err(|_| SettingsError::Malformed)?;
            let value = buf.get_var().map_err(|_| SettingsError::Malformed)?;

            if identifier.is_forbidden() {
                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4.1
                //# Setting identifiers that were defined in [HTTP/2] where there is no
                //# corresponding HTTP/3 setting have also been reserved
                //# (Section 11.2.2).  These reserved settings MUST NOT be sent, and
                //# their receipt MUST be treated as a connection error of type
                //# H3_SETTINGS_ERROR.
                return Err(SettingsError::InvalidSettingId(identifier.0));
            }

            if identifier.is_supported() {
                //= https://www.rfc-editor.org/rfc/rfc9114#section-7.2.4.1
                //# Setting identifiers that were defined in [HTTP/2] where there is no
                //# corresponding HTTP/3 setting have also been reserved
                //# (Section 11.2.2).  These reserved settings MUST NOT be sent, and
                //# their receipt MUST be treated as a connection error of type
                //# H3_SETTINGS_ERROR.
                settings.insert(identifier, value)?;
            }
        }
        Ok(settings)
    }
}

#[derive(Debug, PartialEq)]
pub enum SettingsError {
    Exceeded,
    Malformed,
    Repeated(SettingId),
    InvalidSettingId(u64),
    InvalidSettingValue(SettingId, u64),
}

impl std::error::Error for SettingsError {}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Exceeded => write!(
                f,
                "max settings number exeeded, check for duplicate entries"
            ),
            SettingsError::Malformed => write!(f, "malformed settings frame"),
            SettingsError::Repeated(id) => write!(f, "got setting 0x{:x} twice", id.0),
            SettingsError::InvalidSettingId(id) => write!(f, "setting id 0x{:x} is invalid", id),
            SettingsError::InvalidSettingValue(id, val) => {
                write!(f, "setting 0x{:x} has invalid value {}", id.0, val)
            }
        }
    }
}

impl From<SettingsError> for FrameError {
    fn from(e: SettingsError) -> Self {
        Self::Settings(e)
    }
}

impl From<UnexpectedEnd> for FrameError {
    fn from(e: UnexpectedEnd) -> Self {
        FrameError::Incomplete(e.0)
    }
}

impl From<InvalidStreamId> for FrameError {
    fn from(e: InvalidStreamId) -> Self {
        FrameError::InvalidStreamId(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::io::Cursor;

    #[test]
    fn unknown_frame_type() {
        let mut buf = Cursor::new(&[22, 4, 0, 255, 128, 0, 3, 1, 2]);
        assert_matches!(Frame::decode(&mut buf), Err(FrameError::UnknownFrame(22)));
        assert_matches!(Frame::decode(&mut buf), Ok(Frame::CancelPush(StreamId(2))));
    }

    #[test]
    fn len_unexpected_end() {
        let mut buf = Cursor::new(&[0, 255]);
        let decoded = Frame::decode(&mut buf);
        assert_matches!(decoded, Err(FrameError::Incomplete(3)));
    }

    #[test]
    fn type_unexpected_end() {
        let mut buf = Cursor::new(&[255]);
        let decoded = Frame::decode(&mut buf);
        assert_matches!(decoded, Err(FrameError::Incomplete(2)));
    }

    #[test]
    fn buffer_too_short() {
        let mut buf = Cursor::new(&[4, 4, 0, 255, 128]);
        let decoded = Frame::decode(&mut buf);
        assert_matches!(decoded, Err(FrameError::Incomplete(6)));
    }

    fn codec_frame_check(mut frame: Frame<Bytes>, wire: &[u8], check_frame: Frame<Bytes>) {
        let mut buf = Vec::new();
        frame.encode_with_payload(&mut buf);
        assert_eq!(&buf, &wire);

        let mut read = Cursor::new(&buf);
        let decoded = Frame::decode(&mut read).unwrap();
        assert_eq!(check_frame, decoded);
    }

    #[test]
    fn settings_frame() {
        codec_frame_check(
            Frame::Settings(Settings {
                entries: [
                    (SettingId::MAX_HEADER_LIST_SIZE, 0xfad1),
                    (SettingId::QPACK_MAX_TABLE_CAPACITY, 0xfad2),
                    (SettingId::QPACK_MAX_BLOCKED_STREAMS, 0xfad3),
                    (SettingId(95), 0),
                ],
                len: 4,
            }),
            &[
                4, 18, 6, 128, 0, 250, 209, 1, 128, 0, 250, 210, 7, 128, 0, 250, 211, 64, 95, 0,
            ],
            Frame::Settings(Settings {
                entries: [
                    (SettingId::MAX_HEADER_LIST_SIZE, 0xfad1),
                    (SettingId::QPACK_MAX_TABLE_CAPACITY, 0xfad2),
                    (SettingId::QPACK_MAX_BLOCKED_STREAMS, 0xfad3),
                    // check without the Grease setting because this is ignored
                    (SettingId(0), 0),
                ],
                len: 3,
            }),
        );
    }

    #[test]
    fn settings_frame_emtpy() {
        codec_frame_check(
            Frame::Settings(Settings::default()),
            &[4, 0],
            Frame::Settings(Settings::default()),
        );
    }

    #[test]
    fn data_frame() {
        codec_frame_check(
            Frame::Data(Bytes::from("1234567")),
            &[0, 7, 49, 50, 51, 52, 53, 54, 55],
            Frame::Data(Bytes::from("1234567")),
        );
    }

    #[test]
    fn simple_frames() {
        codec_frame_check(
            Frame::CancelPush(StreamId(2)),
            &[3, 1, 2],
            Frame::CancelPush(StreamId(2)),
        );
        codec_frame_check(
            Frame::Goaway(StreamId(2)),
            &[7, 1, 2],
            Frame::Goaway(StreamId(2)),
        );
        codec_frame_check(
            Frame::MaxPushId(StreamId(2)),
            &[13, 1, 2],
            Frame::MaxPushId(StreamId(2)),
        );
    }

    #[test]
    fn headers_frames() {
        codec_frame_check(
            Frame::headers("TODO QPACK"),
            &[1, 10, 84, 79, 68, 79, 32, 81, 80, 65, 67, 75],
            Frame::headers("TODO QPACK"),
        );
        codec_frame_check(
            Frame::PushPromise(PushPromise {
                id: 134,
                encoded: Bytes::from("TODO QPACK"),
            }),
            &[5, 12, 64, 134, 84, 79, 68, 79, 32, 81, 80, 65, 67, 75],
            Frame::PushPromise(PushPromise {
                id: 134,
                encoded: Bytes::from("TODO QPACK"),
            }),
        );
    }

    #[test]
    fn reserved_frame() {
        let mut raw = vec![];
        VarInt::from_u32(0x21 + 2 * 0x1f).encode(&mut raw);
        raw.extend(&[6, 0, 255, 128, 0, 250, 218]);
        let mut buf = Cursor::new(&raw);
        let decoded = Frame::decode(&mut buf);
        assert_matches!(decoded, Err(FrameError::UnknownFrame(95)));
    }
}
