//! Implements [OpenAPI Callback Object][callback] for operations.
//!
//! [callback]: https://spec.openapis.org/oas/latest.html#callback-object
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use super::PathItem;
use crate::PropMap;

/// Implements [OpenAPI Callback Object][callback].
///
/// A map of possible out-of-band callbacks related to the parent operation. Each value in
/// the map is a [`PathItem`] that describes a set of requests that may be initiated by the API
/// provider and the expected responses. The key value used to identify the [`PathItem`] is an
/// expression, evaluated at runtime, that identifies a URL to use for the callback operation.
///
/// See the OpenAPI spec for the [Callback Object][callback] for more details on the runtime
/// [expression][expression] syntax used as keys.
///
/// [callback]: https://spec.openapis.org/oas/latest.html#callback-object
/// [expression]: https://spec.openapis.org/oas/latest.html#runtime-expressions
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Callback(pub PropMap<String, PathItem>);

impl Callback {
    /// Construct a new empty [`Callback`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the callback contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Insert a runtime expression to [`PathItem`] mapping and return `self`.
    #[must_use]
    pub fn path<S: Into<String>, P: Into<PathItem>>(mut self, expression: S, path_item: P) -> Self {
        self.0.insert(expression.into(), path_item.into());
        self
    }

    /// Insert a runtime expression to [`PathItem`] mapping into the callback.
    pub fn insert<S: Into<String>, P: Into<PathItem>>(&mut self, expression: S, path_item: P) {
        self.0.insert(expression.into(), path_item.into());
    }
}

impl Deref for Callback {
    type Target = PropMap<String, PathItem>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Callback {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Callback {
    type Item = (String, PathItem);
    type IntoIter = <PropMap<String, PathItem> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<S, P> FromIterator<(S, P)> for Callback
where
    S: Into<String>,
    P: Into<PathItem>,
{
    fn from_iter<I: IntoIterator<Item = (S, P)>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;
    use crate::{Operation, PathItemType};

    #[test]
    fn callback_default_is_empty() {
        let callback = Callback::default();
        assert!(callback.is_empty());
    }

    #[test]
    fn callback_serializes_as_map_of_path_items() {
        let callback = Callback::new().path(
            "{$request.body#/callbackUrl}",
            PathItem::new(PathItemType::Post, Operation::new()),
        );

        assert_json_eq!(
            callback,
            json!({
                "{$request.body#/callbackUrl}": {
                    "post": { "responses": {} }
                }
            })
        );
    }
}
