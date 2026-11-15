use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::io;
use std::io::ErrorKind;
use std::str::FromStr;
use std::vec::IntoIter;

/// rbatis bytes use serde_bytes
///
#[derive(Clone, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    /// Construct a new, empty `ByteBuf`.
    pub fn new() -> Self {
        Bytes(Vec::new())
    }

    /// Construct a new, empty `ByteBuf` with the specified capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Bytes(Vec::with_capacity(cap))
    }

    /// Wrap existing bytes in a `ByteBuf`.
    pub fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Unwrap the vector of byte underlying this `ByteBuf`.
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl Debug for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl<Rhs> PartialEq<Rhs> for Bytes
where
    Rhs: ?Sized + AsRef<[u8]>,
{
    fn eq(&self, other: &Rhs) -> bool {
        self.as_ref().eq(other.as_ref())
    }
}

impl<Rhs> PartialOrd<Rhs> for Bytes
where
    Rhs: ?Sized + AsRef<[u8]>,
{
    fn partial_cmp(&self, other: &Rhs) -> Option<Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Bytes(value)
    }
}

impl From<&[u8]> for Bytes {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for Bytes {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl Default for Bytes {
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl Hash for Bytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<'a> IntoIterator for Bytes {
    type Item = u8;
    type IntoIter = IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Bytes {
    type Item = &'a u8;
    type IntoIter = <&'a [u8] as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Bytes {
    type Item = &'a mut u8;
    type IntoIter = <&'a mut [u8] as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl Serialize for Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let inner = serde_bytes::Bytes::new(&self.0);
        inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf = serde_bytes::ByteBuf::deserialize(deserializer)?;
        Ok(Self(buf.into_vec()))
    }
}

#[derive(Debug, Eq, Clone, Copy, PartialEq, PartialOrd)]
pub struct BytesSize(pub i64);

pub const EB: BytesSize = BytesSize(1024 * 1024 * 1024 * 1024 * 1024);
pub const TB: BytesSize = BytesSize(1024 * 1024 * 1024 * 1024);
pub const GB: BytesSize = BytesSize(1024 * 1024 * 1024);
pub const MB: BytesSize = BytesSize(1024 * 1024);
pub const KB: BytesSize = BytesSize(1024);
pub const B: BytesSize = BytesSize(1);

impl BytesSize {
    pub fn display(&self) -> String {
        format!("{}", self)
    }

    pub fn into_inner(self) -> i64 {
        self.0
    }
}

impl Serialize for BytesSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BytesSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = i64::deserialize(deserializer)?;
        Ok(BytesSize(v))
    }
}

impl From<i64> for BytesSize {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl Default for BytesSize {
    fn default() -> Self {
        BytesSize(0)
    }
}

impl Display for BytesSize {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let s = self.0;
        if s < KB.0 {
            write!(f, "{:.2}B", s as f64 / B.0 as f64)
        } else if s < MB.0 {
            write!(f, "{:.2}KB", s as f64 / KB.0 as f64)
        } else if s < GB.0 {
            write!(f, "{:.2}MB", s as f64 / MB.0 as f64)
        } else if s < TB.0 {
            write!(f, "{:.2}GB", s as f64 / GB.0 as f64)
        } else if s < EB.0 {
            write!(f, "{:.2}TB", s as f64 / TB.0 as f64)
        } else {
            write!(f, "{:.2}EB", s as f64 / EB.0 as f64)
        }
    }
}

impl FromStr for BytesSize {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.ends_with("EB") {
            let v: f64 = s
                .trim_end_matches("EB")
                .parse()
                .map_err(|e| io::Error::new(ErrorKind::Other, e))?;
            Ok(BytesSize((v * EB.0 as f64) as i64))
        } else if s.ends_with("TB") {
            let v: f64 = s
                .trim_end_matches("TB")
                .parse()
                .map_err(|e| io::Error::new(ErrorKind::Other, e))?;
            Ok(BytesSize((v * TB.0 as f64) as i64))
        } else if s.ends_with("GB") {
            let v: f64 = s
                .trim_end_matches("GB")
                .parse()
                .map_err(|e| io::Error::new(ErrorKind::Other, e))?;
            Ok(BytesSize((v * GB.0 as f64) as i64))
        } else if s.ends_with("MB") {
            let v: f64 = s
                .trim_end_matches("MB")
                .parse()
                .map_err(|e| io::Error::new(ErrorKind::Other, e))?;
            Ok(BytesSize((v * MB.0 as f64) as i64))
        } else if s.ends_with("KB") {
            let v: f64 = s
                .trim_end_matches("KB")
                .parse()
                .map_err(|e| io::Error::new(ErrorKind::Other, e))?;
            Ok(BytesSize((v * KB.0 as f64) as i64))
        } else {
            let v: f64 = s
                .trim_end_matches("B")
                .parse()
                .map_err(|e| io::Error::new(ErrorKind::Other, e))?;
            Ok(BytesSize((v * B.0 as f64) as i64))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::bytes::GB;

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", GB), "1.00GB");
    }
}
