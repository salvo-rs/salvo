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
