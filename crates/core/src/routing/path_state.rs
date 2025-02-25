use std::borrow::Cow;

use super::{PathParams, decode_url_path_safely};

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathState {
    pub(crate) parts: Vec<String>,
    /// (row, col), row is the index of parts, col is the index of char in the part.
    pub(crate) cursor: (usize, usize),
    pub(crate) params: PathParams,
    #[cfg(feature = "matched-path")]
    pub(crate) matched_parts: Vec<String>,
    pub(crate) end_slash: bool, // For rest match, we want include the last slash.
    pub(crate) once_ended: bool, // Once it has ended, used to determine whether the error code returned is 404 or 405.
}
impl PathState {
    /// Creates a new `PathState`.
    #[inline]
    pub fn new(url_path: &str) -> Self {
        let end_slash = url_path.ends_with('/');
        let parts = url_path
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter_map(|p| {
                if !p.is_empty() {
                    Some(decode_url_path_safely(p))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        PathState {
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
    pub fn pick(&self) -> Option<&str> {
        match self.parts.get(self.cursor.0) {
            None => None,
            Some(part) => {
                if self.cursor.1 >= part.len() {
                    let row = self.cursor.0 + 1;
                    self.parts.get(row).map(|s| &**s)
                } else {
                    Some(&part[self.cursor.1..])
                }
            }
        }
    }

    #[inline]
    pub fn all_rest(&self) -> Option<Cow<'_, str>> {
        if let Some(picked) = self.pick() {
            if self.cursor.0 >= self.parts.len() - 1 {
                if self.end_slash {
                    Some(Cow::Owned(format!("{picked}/")))
                } else {
                    Some(Cow::Borrowed(picked))
                }
            } else {
                let last = self.parts[self.cursor.0 + 1..].join("/");
                if self.end_slash {
                    Some(Cow::Owned(format!("{picked}/{last}/")))
                } else {
                    Some(Cow::Owned(format!("{picked}/{last}")))
                }
            }
        } else {
            None
        }
    }

    #[inline]
    pub fn forward(&mut self, steps: usize) {
        let mut steps = steps + self.cursor.1;
        while let Some(part) = self.parts.get(self.cursor.0) {
            if part.len() > steps {
                self.cursor.1 = steps;
                return;
            } else {
                steps -= part.len();
                self.cursor = (self.cursor.0 + 1, 0);
            }
        }
    }

    #[inline]
    pub fn is_ended(&self) -> bool {
        self.cursor.0 >= self.parts.len()
    }
}
