use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::Range;

use super::{PathParams, decode_url_path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PathPart {
    Raw(Range<usize>),
    Decoded(String),
}
impl PathPart {
    #[inline]
    fn as_str<'a>(&'a self, source: &'a str) -> &'a str {
        match self {
            Self::Raw(range) => &source[range.clone()],
            Self::Decoded(value) => value,
        }
    }

    #[inline]
    fn len(&self, source: &str) -> usize {
        self.as_str(source).len()
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathState<'a> {
    path: String,
    pub(crate) parts: Vec<PathPart>,
    /// (row, col), row is the index of parts, col is the index of char in the part.
    pub(crate) cursor: (usize, usize),
    pub(crate) params: PathParams,
    #[cfg(feature = "matched-path")]
    pub(crate) matched_parts: Vec<String>,
    pub(crate) end_slash: bool, // For rest match, we want include the last slash.
    pub(crate) once_ended: bool, /* Once it has ended, used to determine whether the error code
                                 * returned is 404 or 405. */
    marker: PhantomData<&'a ()>,
}
impl<'a> PathState<'a> {
    /// Creates a new `PathState`.
    #[inline]
    #[must_use]
    pub fn new(url_path: &str) -> Self {
        Self::from_path(url_path.to_owned())
    }

    /// Creates a new `PathState` from an owned path.
    #[inline]
    #[must_use]
    pub fn from_path(path: String) -> Self {
        let end_slash = path.ends_with('/');
        let parts = parse_path_parts(&path);
        Self {
            path,
            parts,
            cursor: (0, 0),
            params: PathParams::new(),
            end_slash,
            once_ended: false,
            #[cfg(feature = "matched-path")]
            matched_parts: vec![],
            marker: PhantomData,
        }
    }

    #[inline]
    #[must_use]
    pub fn pick(&self) -> Option<&str> {
        match self.parts.get(self.cursor.0) {
            None => None,
            Some(part) => {
                let part = part.as_str(&self.path);
                if self.cursor.1 >= part.len() {
                    let row = self.cursor.0 + 1;
                    self.parts.get(row).map(|s| s.as_str(&self.path))
                } else {
                    part.get(self.cursor.1..)
                }
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn all_rest(&self) -> Option<Cow<'_, str>> {
        if let Some(picked) = self.pick() {
            if self.cursor.0 >= self.parts.len() - 1 {
                if self.end_slash {
                    Some(Cow::Owned(format!("{picked}/")))
                } else {
                    Some(Cow::Borrowed(picked))
                }
            } else {
                let rest = &self.parts[self.cursor.0 + 1..];
                let trailing = usize::from(self.end_slash);
                let cap = picked.len()
                    + rest.iter().map(|s| s.len(&self.path) + 1).sum::<usize>()
                    + trailing;
                let mut buf = String::with_capacity(cap);
                buf.push_str(picked);
                for part in rest {
                    buf.push('/');
                    buf.push_str(part.as_str(&self.path));
                }
                if self.end_slash {
                    buf.push('/');
                }
                Some(Cow::Owned(buf))
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn forward(&mut self, steps: usize) {
        let mut steps = steps + self.cursor.1;
        while let Some(part) = self.parts.get(self.cursor.0) {
            let len = part.len(&self.path);
            if len > steps {
                self.cursor.1 = steps;
                return;
            } else {
                steps -= len;
                self.cursor = (self.cursor.0 + 1, 0);
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.cursor.0 >= self.parts.len()
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn parts(&self) -> impl Iterator<Item = &str> + '_ {
        self.parts.iter().map(|part| part.as_str(&self.path))
    }
}

fn parse_path_parts(path: &str) -> Vec<PathPart> {
    let mut parts = Vec::new();
    let end = path.trim_end_matches('/').len();
    let mut cursor = path.len() - path.trim_start_matches('/').len();
    while cursor < end {
        while cursor < end && path.as_bytes()[cursor] == b'/' {
            cursor += 1;
        }
        let start = cursor;
        while cursor < end && path.as_bytes()[cursor] != b'/' {
            cursor += 1;
        }
        if start == cursor {
            continue;
        }
        match decode_url_path(&path[start..cursor]) {
            Cow::Borrowed(_) => parts.push(PathPart::Raw(start..cursor)),
            Cow::Owned(decoded) => parts.push(PathPart::Decoded(decoded)),
        }
    }
    parts
}
