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
    /// Create new `PathParams`.
    pub fn new() -> Self {
        PathParams::default()
    }
    /// If there is a wildcard param, its value is `true`.
    pub fn greedy(&self) -> bool {
        self.greedy
    }
    /// Get the last param starts with '*', for example: <**rest>, <*?rest>.
    pub fn tail(&self) -> Option<&str> {
        if self.greedy {
            self.inner.last().map(|(_, v)| &**v)
        } else {
            None
        }
    }

    /// Insert new param.
    pub fn insert(&mut self, name: &str, value: String) {
        #[cfg(debug_assertions)]
        {
            if self.greedy {
                panic!("only one wildcard param is allowed and it must be the last one.");
            }
        }
        if name.starts_with('*') {
            self.inner.insert(split_wild_name(name).1.to_owned(), value);
            self.greedy = true;
        } else {
            self.inner.insert(name.to_owned(), value);
        }
    }
}
