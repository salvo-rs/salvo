use std::fmt::{self, Debug, Formatter};
use std::ops::{Deref, Range};

use indexmap::IndexMap;

use super::split_wild_name;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapturedPath {
    pub(crate) index: usize,
    pub(crate) ranges: Vec<Range<usize>>,
    pub(crate) encoded: String,
}

impl CapturedPath {
    fn from_decoded(path: &str) -> Self {
        use std::fmt::Write as _;
        let mut offset = 0;
        let ranges = path
            .split('/')
            .map(|part| {
                let range = offset..offset + part.len();
                offset += part.len() + 1;
                range
            })
            .collect();
        let mut encoded = String::with_capacity(path.len());
        for (index, part) in path.split('/').enumerate() {
            if index > 0 {
                encoded.push('/');
            }
            let _ = write!(
                encoded,
                "{}",
                percent_encoding::utf8_percent_encode(part, percent_encoding::NON_ALPHANUMERIC)
            );
        }
        Self {
            index: 0,
            ranges,
            encoded,
        }
    }
}

/// The path parameters.
#[derive(Clone, Default)]
pub struct PathParams {
    inner: IndexMap<String, String>,
    greedy: bool,
    tail_path: Option<CapturedPath>,
    /// Transaction log of values overwritten by [`insert`](Self::insert), used to
    /// undo in-place overwrites during [`rollback`](Self::rollback). Kept out of the
    /// public identity of `PathParams` (`Debug`/`PartialEq`/`Eq`) because it is
    /// transient routing state, not observable path data.
    rollback_log: Vec<(usize, String)>,
}
// Captured values and segment provenance are observable; the rollback log is
// internal bookkeeping, so it is excluded from these impls.
impl Debug for PathParams {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathParams")
            .field("inner", &self.inner)
            .field("greedy", &self.greedy)
            .field("tail_path", &self.tail_path)
            .finish()
    }
}
impl PartialEq for PathParams {
    fn eq(&self, other: &Self) -> bool {
        self.greedy == other.greedy
            && self.inner == other.inner
            && self.tail_path == other.tail_path
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
    /// Get the decoded wildcard value, for example `{**rest}` or `{*?rest}`.
    ///
    /// This string does not distinguish encoded separators inside a segment from
    /// separators between segments. Resource handlers should use [`Self::tail_segments`].
    #[must_use]
    pub fn tail(&self) -> Option<&str> {
        self.tail_path
            .as_ref()
            .and_then(|path| self.inner.get_index(path.index))
            .map(|(_, value)| value.as_str())
    }

    /// Returns the wildcard's decoded segments with their routing boundaries intact.
    ///
    /// A slash decoded from `%2F` remains inside one item. A trailing slash is
    /// represented by a final empty item. Values are decoded exactly once.
    /// For wildcard values inserted through [`Self::insert`], literal slashes in
    /// the supplied value explicitly delimit segments.
    #[must_use]
    pub fn tail_segments(&self) -> Option<impl ExactSizeIterator<Item = &str>> {
        let value = self.tail()?;
        Some(
            self.tail_path
                .as_ref()?
                .ranges
                .iter()
                .map(move |range| &value[range.clone()]),
        )
    }

    /// Returns the wildcard's encoded segments before parameter decoding.
    ///
    /// Routing may remove dot and empty segments; use the request URI itself for
    /// signature verification. Manually inserted values are percent-encoded.
    #[must_use]
    pub fn tail_raw_segments(&self) -> Option<impl Iterator<Item = &str>> {
        self.tail_path
            .as_ref()
            .map(|path| path.encoded.split('/').take(path.ranges.len()))
    }

    /// Snapshot the current state so it can be rolled back after a failed match attempt.
    ///
    /// During routing a child router may capture params and then fail to match, in which
    /// case its captures must be discarded before the next sibling is tried. Matching only
    /// mutates params through [`insert_tracked`](Self::insert_tracked), which either
    /// appends a new entry or overwrites an existing one in place; the latter records the
    /// previous value in `rollback_log`. So the pre-descent state is fully described by
    /// `(len, greedy, rollback_log.len())` — three `Copy` values. Rolling back is `O(1)`
    /// on the common append-only path; overwrites are undone by an `O(k)` replay of the
    /// `k` logged entries. Either way no map clone is needed.
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
            } else {
                // Every logged index points at an entry that existed when the overwrite
                // happened, and entries logged after the snapshot are replayed before
                // `truncate` runs — so this branch is unreachable unless the log and the
                // map fall out of sync. Surface that loudly in debug builds.
                debug_assert!(
                    false,
                    "rollback log index {index} out of bounds; log is inconsistent with params"
                );
            }
        }
        self.inner.truncate(len);
        self.greedy = greedy;
        if !greedy {
            self.tail_path = None;
        }
    }

    /// Clear the rollback log once route matching has finished.
    ///
    /// After a successful match the log has served its purpose; dropping it before the
    /// params are handed to the `Request` keeps overwritten values from lingering for the
    /// rest of the request lifetime.
    #[inline]
    pub(crate) fn seal(&mut self) {
        self.rollback_log = Vec::new();
    }

    /// Insert new param.
    ///
    /// Overwriting an existing key keeps constant space: no rollback bookkeeping is
    /// recorded here. Route matching uses [`insert_tracked`](Self::insert_tracked)
    /// internally instead, which logs overwrites so failed match attempts can be undone.
    pub fn insert(&mut self, name: &str, value: String) {
        self.insert_inner(name, value, false, None);
    }

    /// Insert a param during route matching, recording any overwritten value so
    /// [`rollback`](Self::rollback) can restore it after a failed match attempt.
    pub(crate) fn insert_tracked(&mut self, name: &str, value: String) {
        self.insert_inner(name, value, true, None);
    }

    pub(crate) fn insert_tail_tracked(&mut self, name: &str, value: String, path: CapturedPath) {
        self.insert_inner(name, value, true, Some(path));
    }

    fn insert_inner(&mut self, name: &str, value: String, track: bool, path: Option<CapturedPath>) {
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
            self.tail_path = Some(path.unwrap_or_else(|| CapturedPath::from_decoded(&value)));
            self.inner
                .insert_full(split_wild_name(name).1.to_owned(), value)
        } else {
            self.inner.insert_full(name.to_owned(), value)
        };
        if let Some(path) = &mut self.tail_path {
            path.index = index;
        }
        // If this overwrote a param captured earlier (a descendant reusing an ancestor's
        // name), remember the old value so `rollback` can restore it on a failed match.
        if track && let Some(old) = replaced {
            self.rollback_log.push((index, old));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PathParams;

    #[test]
    fn segment_iterators_support_skipping_and_last() {
        let mut params = PathParams::new();
        params.insert("**rest", "first/second/third/".to_owned());
        assert_eq!(params.tail_segments().unwrap().nth(1), Some("second"));
        assert_eq!(
            params.tail_segments().unwrap().skip(1).collect::<Vec<_>>(),
            ["second", "third", ""]
        );
        assert_eq!(params.tail_segments().unwrap().last(), Some(""));
        assert_eq!(params.tail_raw_segments().unwrap().nth(2), Some("third"));
    }

    #[test]
    fn test_public_insert_keeps_constant_space() {
        // Overwriting through the public API must not accumulate rollback state:
        // users can hold `PathParams` (via `req.params_mut()`) long after routing.
        let mut params = PathParams::new();
        for i in 0..100 {
            params.insert("key", format!("value-{i}"));
        }
        assert!(params.rollback_log.is_empty());
        assert_eq!(params.get("key").map(String::as_str), Some("value-99"));
    }

    #[test]
    fn test_tracked_insert_logs_and_seal_clears() {
        let mut params = PathParams::new();
        params.insert_tracked("key", "a".into());
        assert!(params.rollback_log.is_empty());
        params.insert_tracked("key", "b".into());
        assert_eq!(params.rollback_log.len(), 1);

        params.seal();
        assert!(params.rollback_log.is_empty());
        assert_eq!(params.get("key").map(String::as_str), Some("b"));
    }

    #[test]
    fn test_rollback_restores_overwritten_value() {
        let mut params = PathParams::new();
        params.insert_tracked("id", "alice".into());
        let snapshot = params.snapshot();
        params.insert_tracked("id", "edit".into());
        params.insert_tracked("extra", "x".into());
        params.rollback(snapshot);
        assert_eq!(params.get("id").map(String::as_str), Some("alice"));
        assert!(!params.contains_key("extra"));
        assert!(params.rollback_log.is_empty());
    }
}
