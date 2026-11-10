use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::ops::Range;

use super::{PathParams, decode_url_path, is_invalid_path_segment};

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
    valid_path: bool,
}

/// A lightweight transaction boundary for path matching.
#[derive(Clone, Copy)]
pub(crate) struct PathCheckpoint {
    cursor: (usize, usize),
    params: (usize, bool, usize),
    end_slash: bool,
    #[cfg(feature = "matched-path")]
    matched_parts_len: usize,
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
            .field("valid_path", &self.valid_path)
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
            || self.valid_path != other.valid_path
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
    pub(crate) fn checkpoint(&self) -> PathCheckpoint {
        PathCheckpoint {
            cursor: self.cursor,
            params: self.params.snapshot(),
            end_slash: self.end_slash,
            #[cfg(feature = "matched-path")]
            matched_parts_len: self.matched_parts.len(),
        }
    }

    pub(crate) fn rollback(&mut self, checkpoint: PathCheckpoint) {
        self.cursor = checkpoint.cursor;
        self.params.rollback(checkpoint.params);
        self.end_slash = checkpoint.end_slash;
        #[cfg(feature = "matched-path")]
        self.matched_parts.truncate(checkpoint.matched_parts_len);
        // `once_ended` accumulates path matches across failed router branches
        // to distinguish 405 from 404, so it is intentionally not rolled back.
    }

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
        let (parts, valid_path) = parse_path_parts(path.as_ref());
        Self {
            path,
            parts,
            cursor: (0, 0),
            params: PathParams::new(),
            end_slash,
            once_ended: false,
            valid_path,
            #[cfg(feature = "matched-path")]
            matched_parts: vec![],
        }
    }

    #[inline]
    pub(crate) fn is_valid_path(&self) -> bool {
        self.valid_path
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

fn parse_path_parts(path: &str) -> (Vec<PathPart>, bool) {
    let mut parts = Vec::new();
    let mut valid_path = true;
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
        let decoded = decode_url_path(&path[start..cursor]);
        if decoded
            .as_bytes()
            .iter()
            .any(|byte| matches!(byte, b'/' | b'\\'))
            || is_invalid_path_segment(&decoded)
        {
            valid_path = false;
        }
        match decoded.as_ref() {
            "." => {}
            ".." => {
                parts.pop();
            }
            _ => match decoded {
                Cow::Borrowed(_) => parts.push(PathPart::Raw(start..cursor)),
                Cow::Owned(decoded) => parts.push(PathPart::Decoded(decoded)),
            },
        }
    }
    (parts, valid_path)
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
            PathState::from_borrowed_path("/%E7%94%A8%E6%88%B7/%F0%9F%98%80/a%20b/rest");

        assert_eq!(
            state.parts().collect::<Vec<_>>(),
            vec![user, emoji, "a b", "rest"]
        );
        assert_eq!(state.pick(), Some(user));

        state.forward(user.len());
        assert_eq!(state.pick(), Some(emoji));

        state.forward(emoji.len());
        assert_eq!(state.pick(), Some("a b"));

        state.forward("a b".len());
        assert_eq!(state.pick(), Some("rest"));
    }

    #[test]
    fn rejects_encoded_and_literal_path_separators() {
        for path in [
            "/%2freport_note/secret.txt",
            "/%2Freport_note/secret.txt",
            "/%5creport_note/secret.txt",
            "/%5Creport_note/secret.txt",
            "/report_note%2fsecret.txt",
            "/report_note%5csecret.txt",
            r"/\report_note/secret.txt",
            r"/report_note\secret.txt",
        ] {
            assert!(
                !PathState::from_borrowed_path(path).is_valid_path(),
                "{path}"
            );
        }
    }

    #[test]
    fn normalizes_dot_segments_after_decoding() {
        for path in [
            "/./report_note/secret.txt",
            "/%2e/report_note/secret.txt",
            "/%2E/report_note/secret.txt",
            "/public/../report_note/secret.txt",
            "/public/%2e%2e/report_note/secret.txt",
            "/public/%2E%2E/report_note/secret.txt",
        ] {
            let state = PathState::from_borrowed_path(path);
            assert!(state.is_valid_path(), "{path}");
            assert_eq!(
                state.parts().collect::<Vec<_>>(),
                vec!["report_note", "secret.txt"],
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_segments_discarded_by_downstream_normalization() {
        for path in [
            "/%00/report_note/secret.txt",
            "/ignored%00segment/report_note/secret.txt",
            "/%00/../report_note/secret.txt",
            "/report_note/secret.txt%00",
            "/\0/report_note/secret.txt",
        ] {
            assert!(
                !PathState::from_borrowed_path(path).is_valid_path(),
                "{path}"
            );
        }
        for part in [
            "C:",
            "C%3a",
            "CON",
            "%43%4f%4e",
            "NUL.txt",
            "pRn",
            "AUX",
            "COM1",
            "LPT9",
        ] {
            let path = format!("/{part}/report_note/secret.txt");
            let state = PathState::from_borrowed_path(&path);
            assert_eq!(state.is_valid_path(), !cfg!(windows), "{path}");
        }
    }

    #[test]
    fn accepted_paths_keep_the_same_downstream_namespace() {
        use super::super::{decode_url_path, normalize_url_path};

        // Exercise every transformation that downstream path normalization
        // performs, including components removed by a later parent segment.
        for prefix in [
            "",
            "public/..",
            ".",
            "%2e",
            "%2E%2E",
            "%00",
            "x%00y",
            "C:",
            "CON.txt",
            "COM1",
            "%2f",
            "%5c",
            "%00/..",
            "CON/..",
            "a//..",
            "CONSOLE",
            "COM10",
        ] {
            for suffix in ["/report_note/secret.txt", "/report_note/secret.txt/"] {
                let path = format!("/{prefix}{suffix}");
                let state = PathState::from_borrowed_path(&path);
                if state.is_valid_path() {
                    assert_eq!(
                        state.all_rest().unwrap_or_default(),
                        normalize_url_path(&decode_url_path(&path)),
                        "{path}"
                    );
                }
            }
        }
    }

    #[test]
    fn combined_path_components_keep_routing_and_downstream_in_agreement() {
        use super::super::{decode_url_path, normalize_url_path};

        // Combine components in different positions so parent references, empty
        // paths, UTF-8 byte ranges, and invalid components cannot mask each other.
        let components = [
            "",
            "a",
            "report_note",
            ".",
            "..",
            "%2e",
            "%2e%2E",
            "%00",
            "a%00b",
            "%2f",
            "%5C",
            "%252f",
            "C:",
            "CON.txt",
            "COM10",
            "%E7%94%A8%E6%88%B7",
        ];
        for first in components {
            for second in components {
                for third in components {
                    for trailing in ["", "/"] {
                        let path = format!("/{first}/{second}/{third}{trailing}");
                        let state = PathState::from_borrowed_path(&path);
                        if state.is_valid_path() {
                            let expected = normalize_url_path(&decode_url_path(&path));
                            let rest = state.all_rest().unwrap_or_default();
                            // The empty root has no captured tail, even when the
                            // URI ends with '/'. Nonempty tails preserve it.
                            if state.is_ended() {
                                assert!(expected.is_empty() || expected == "/", "{path}");
                            } else {
                                assert_eq!(rest, expected, "{path}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn preserves_non_structural_percent_decoding() {
        for (path, expected) in [
            ("/%2eenv", ".env"),
            ("/v1%2e2", "v1.2"),
            ("/hello%20world", "hello world"),
            ("/%252f", "%2f"),
        ] {
            let state = PathState::from_borrowed_path(path);
            assert!(state.is_valid_path(), "{path}");
            assert_eq!(state.parts().collect::<Vec<_>>(), vec![expected], "{path}");
        }
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
