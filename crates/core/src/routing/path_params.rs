use std::ops::Deref;

use indexmap::IndexMap;

use super::split_wild_name;

/// The path parameters.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct PathParams {
    inner: IndexMap<String, String>,
    greedy: bool,
}
impl Deref for PathParams {
    type Target = IndexMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl PathParams {
    /// Creates a new `PathParams`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// If there is a wildcard param, its value is `true`.
    #[must_use]
    pub fn greedy(&self) -> bool {
        self.greedy
    }
    /// Get the last param starts with '*', for example: <**rest>, <*?rest>.
    #[must_use]
    pub fn tail(&self) -> Option<&str> {
        if self.greedy {
            self.inner.last().map(|(_, v)| &**v)
        } else {
            None
        }
    }

    /// Snapshot the current state so it can be rolled back after a failed match attempt.
    ///
    /// During routing a child router may capture params and then fail to match, in which
    /// case its captures must be discarded before the next sibling is tried. Because
    /// matching only ever *appends* params via [`insert`](Self::insert), the state is fully
    /// described by the current length plus the `greedy` flag, so a snapshot is just two
    /// `Copy` values and rolling back is `O(1)` instead of cloning the whole map.
    #[inline]
    pub(crate) fn snapshot(&self) -> (usize, bool) {
        (self.inner.len(), self.greedy)
    }

    /// Roll back to a state previously captured by [`snapshot`](Self::snapshot).
    #[inline]
    pub(crate) fn rollback(&mut self, (len, greedy): (usize, bool)) {
        self.inner.truncate(len);
        self.greedy = greedy;
    }

    /// Insert new param.
    pub fn insert(&mut self, name: &str, value: String) {
        if self.greedy {
            // A wildcard param must be the last one. Reaching here means an earlier
            // wildcard was not rolled back correctly. In debug builds this is a bug
            // worth catching loudly; in release we must not silently corrupt the
            // existing params (the previous code kept inserting), so drop the stray
            // insert and log instead.
            debug_assert!(
                false,
                "only one wildcard param is allowed and it must be the last one"
            );
            tracing::error!(
                param = name,
                "ignoring a param inserted after a wildcard param; this indicates a routing bug"
            );
            return;
        }
        if name.starts_with('*') {
            self.inner.insert(split_wild_name(name).1.to_owned(), value);
            self.greedy = true;
        } else {
            self.inner.insert(name.to_owned(), value);
        }
    }
}
