use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;

use indexmap::IndexMap;

use super::split_wild_name;

/// The path parameters.
#[derive(Clone, Default)]
pub struct PathParams {
    inner: IndexMap<String, String>,
    greedy: bool,
    /// Transaction log of values overwritten by [`insert`](Self::insert), used to
    /// undo in-place overwrites during [`rollback`](Self::rollback). Kept out of the
    /// public identity of `PathParams` (`Debug`/`PartialEq`/`Eq`) because it is
    /// transient routing state, not observable path data.
    rollback_log: Vec<(usize, String)>,
}
// Only `inner` + `greedy` define the observable value of `PathParams`; `rollback_log`
// is internal bookkeeping, so it is excluded from these impls.
impl Debug for PathParams {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathParams")
            .field("inner", &self.inner)
            .field("greedy", &self.greedy)
            .finish()
    }
}
impl PartialEq for PathParams {
    fn eq(&self, other: &Self) -> bool {
        self.greedy == other.greedy && self.inner == other.inner
    }
}
impl Eq for PathParams {}
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
    /// case its captures must be discarded before the next sibling is tried. Matching only
    /// mutates params through [`insert`](Self::insert), which either appends a new entry or
    /// overwrites an existing one in place; the latter records the previous value in
    /// `rollback_log`. So the pre-descent state is fully described by
    /// `(len, greedy, rollback_log.len())` — three `Copy` values — and rolling back is
    /// `O(1)` amortized instead of cloning the whole map on every sibling mismatch.
    #[inline]
    pub(crate) fn snapshot(&self) -> (usize, bool, usize) {
        (self.inner.len(), self.greedy, self.rollback_log.len())
    }

    /// Roll back to a state previously captured by [`snapshot`](Self::snapshot).
    ///
    /// First replays the overwrite log (newest first) to restore any ancestor values that a
    /// failed descendant clobbered, then drops the entries appended since the snapshot.
    /// Restoring before truncating keeps every logged index valid.
    #[inline]
    pub(crate) fn rollback(&mut self, (len, greedy, log_len): (usize, bool, usize)) {
        while self.rollback_log.len() > log_len {
            let (index, old) = self
                .rollback_log
                .pop()
                .expect("rollback_log length was checked");
            if let Some((_, value)) = self.inner.get_index_mut(index) {
                *value = old;
            }
        }
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
        let (index, replaced) = if name.starts_with('*') {
            self.greedy = true;
            self.inner
                .insert_full(split_wild_name(name).1.to_owned(), value)
        } else {
            self.inner.insert_full(name.to_owned(), value)
        };
        // If this overwrote a param captured earlier (a descendant reusing an ancestor's
        // name), remember the old value so `rollback` can restore it on a failed match.
        if let Some(old) = replaced {
            self.rollback_log.push((index, old));
        }
    }
}
