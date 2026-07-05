use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::ops::Range;

use super::{PathParams, decode_url_path};

#[derive(Clone, Debug, Eq, PartialEq)]
enum PathPart {
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
#[derive(Clone)]
pub struct PathState<'a> {
    path: Cow<'a, str>,
    parts: Vec<PathPart>,
    /// (row, col), row is the index of parts, col is the index of char in the part.
    pub(crate) cursor: (usize, usize),
    pub(crate) params: PathParams,
    #[cfg(feature = "matched-path")]
    pub(crate) matched_parts: Vec<String>,
    pub(crate) end_slash: bool, // For rest match, we want include the last slash.
    pub(crate) once_ended: bool, /* Once it has ended, used to determine whether the error code
                                 * returned is 404 or 405. */
}
impl<'a> Debug for PathState<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let parts = self.parts_iter().collect::<Vec<_>>();
        let mut debug = f.debug_struct("PathState");
        debug
            .field("parts", &parts)
            .field("cursor", &self.cursor)
            .field("params", &self.params);
        #[cfg(feature = "matched-path")]
        debug.field("matched_parts", &self.matched_parts);
        debug
            .field("end_slash", &self.end_slash)
            .field("once_ended", &self.once_ended)
            .finish()
    }
}
impl<'a, 'b> PartialEq<PathState<'b>> for PathState<'a> {
    fn eq(&self, other: &PathState<'b>) -> bool {
        if !self.parts_iter().eq(other.parts_iter()) {
            return false;
        }
        if self.cursor != other.cursor
            || self.params != other.params
            || self.end_slash != other.end_slash
            || self.once_ended != other.once_ended
        {
            return false;
        }
        #[cfg(feature = "matched-path")]
        if self.matched_parts != other.matched_parts {
            return false;
        }
        true
    }
}
impl<'a> Eq for PathState<'a> {}
impl<'a> PathState<'a> {
    /// Creates a new owned `PathState` from a borrowed path.
    #[inline]
    #[must_use]
    #[deprecated(
        since = "0.93.0",
        note = "use PathState::from_owned_path or PathState::from_borrowed_path to make ownership explicit"
    )]
    pub fn new(url_path: &str) -> Self {
        Self::from_owned_path(url_path.to_owned())
    }

    /// Creates a new `PathState` by borrowing the supplied path.
    ///
    /// Use this only when the borrowed path outlives the routing operation and is
    /// not borrowed from the same [`Request`](crate::http::Request) that will be
    /// passed mutably to router detection.
    #[inline]
    #[must_use]
    pub fn from_borrowed_path(path: &'a str) -> Self {
        Self::from_cow_path(Cow::Borrowed(path))
    }

    /// Creates a new `PathState` from an owned path without copying it.
    #[inline]
    #[must_use]
    pub fn from_owned_path(path: String) -> Self {
        Self::from_cow_path(Cow::Owned(path))
    }

    /// Creates a new `PathState` from a copy-on-write path.
    #[inline]
    #[must_use]
    pub fn from_cow_path(path: Cow<'a, str>) -> Self {
        let end_slash = path.ends_with('/');
        let parts = parse_path_parts(path.as_ref());
        Self {
            path,
            parts,
            cursor: (0, 0),
            params: PathParams::new(),
            end_slash,
            once_ended: false,
            #[cfg(feature = "matched-path")]
            matched_parts: vec![],
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
    pub(crate) fn parts_len(&self) -> usize {
        self.parts.len()
    }

    #[inline]
    fn parts_iter(&self) -> impl Iterator<Item = &str> + '_ {
        self.parts.iter().map(|part| part.as_str(&self.path))
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn parts(&self) -> impl Iterator<Item = &str> + '_ {
        self.parts_iter()
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::PathState;

    #[test]
    fn constructors_make_path_ownership_explicit() {
        let borrowed = PathState::from_borrowed_path("plain");
        assert!(matches!(borrowed.path, Cow::Borrowed("plain")));

        let owned = PathState::from_owned_path("plain".to_owned());
        assert!(matches!(owned.path, Cow::Owned(ref path) if path == "plain"));

        let cow = PathState::from_cow_path(Cow::Borrowed("plain"));
        assert!(matches!(cow.path, Cow::Borrowed("plain")));
    }

    #[test]
    fn raw_utf8_ranges_stay_on_char_boundaries() {
        let user = "\u{7528}\u{6237}";
        let emoji = "\u{1f600}";
        let e_accent = "\u{e9}";
        let path = format!("/{user}/{emoji}/{e_accent}/rest/");
        let mut state = PathState::from_borrowed_path(&path);

        assert_eq!(
            state.parts().collect::<Vec<_>>(),
            vec![user, emoji, e_accent, "rest"]
        );
        assert_eq!(state.pick(), Some(user));

        state.forward(user.len());
        assert_eq!(state.pick(), Some(emoji));

        state.forward(emoji.len());
        assert_eq!(state.pick(), Some(e_accent));
        assert_eq!(state.all_rest().as_deref(), Some("\u{e9}/rest/"));
    }

    #[test]
    fn decoded_utf8_parts_advance_by_bytes() {
        let user = "\u{7528}\u{6237}";
        let emoji = "\u{1f600}";
        let mut state =
            PathState::from_borrowed_path("/%E7%94%A8%E6%88%B7/%F0%9F%98%80/a%2Fb/rest");

        assert_eq!(
            state.parts().collect::<Vec<_>>(),
            vec![user, emoji, "a/b", "rest"]
        );
        assert_eq!(state.pick(), Some(user));

        state.forward(user.len());
        assert_eq!(state.pick(), Some(emoji));

        state.forward(emoji.len());
        assert_eq!(state.pick(), Some("a/b"));

        state.forward("a/b".len());
        assert_eq!(state.pick(), Some("rest"));
    }

    #[test]
    fn equality_uses_decoded_parts_not_internal_storage() {
        let user = "\u{7528}\u{6237}";
        assert_eq!(
            PathState::from_borrowed_path(user),
            PathState::from_owned_path(format!("/{user}"))
        );
        assert_eq!(
            PathState::from_borrowed_path(user),
            PathState::from_borrowed_path("%E7%94%A8%E6%88%B7")
        );
    }
}
