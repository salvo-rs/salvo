use std::any::TypeId;
use std::collections::{BTreeSet, HashMap};
use std::sync::{LazyLock, RwLock};

use salvo_core::Router;

use crate::SecurityRequirement;
use crate::path::PathItemType;

fn normalize_oapi_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    let mut chars = path.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if ch != '{' {
            normalized.push(ch);
            continue;
        }
        // Keep escaped literal braces (`{{`) as-is.
        if chars.peek().map(|(_, next)| *next) == Some('{') {
            normalized.push('{');
            normalized.push('{');
            chars.next();
            continue;
        }

        let content_start = start + ch.len_utf8();
        let mut braces_depth = 0usize;
        let mut escaping = false;
        let mut param_end = None;

        while let Some((idx, current)) = chars.next() {
            if escaping {
                escaping = false;
                continue;
            }
            match current {
                '\\' => escaping = true,
                '{' => braces_depth += 1,
                '}' => {
                    if braces_depth == 0 {
                        param_end = Some(idx);
                        break;
                    }
                    braces_depth -= 1;
                }
                _ => {}
            }
        }

        if let Some(param_end) = param_end {
            let content = &path[content_start..param_end];
            if let Some(name_end) = content.find([':', '|']) {
                normalized.push('{');
                normalized.push_str(&content[..name_end]);
                normalized.push('}');
            } else {
                normalized.push('{');
                normalized.push_str(content);
                normalized.push('}');
            }
        } else {
            normalized.push_str(&path[start..]);
            break;
        }
    }
    normalized
}

#[derive(Debug, Default)]
pub(crate) struct NormNode {
    // pub(crate) router_id: usize,
    pub(crate) handler_type_id: Option<TypeId>,
    pub(crate) handler_type_name: Option<&'static str>,
    pub(crate) method: Option<PathItemType>,
    pub(crate) path: Option<String>,
    pub(crate) children: Vec<Self>,
    pub(crate) metadata: Metadata,
}

impl NormNode {
    pub(crate) fn new(router: &Router, inherited_metadata: Metadata) -> Self {
        let mut node = Self {
            // router_id: router.id,
            metadata: inherited_metadata,
            ..Self::default()
        };
        let registry = METADATA_REGISTRY
            .read()
            .expect("failed to lock METADATA_REGISTRY for read");
        if let Some(metadata) = registry.get(&router.id) {
            node.metadata.tags.extend(metadata.tags.iter().cloned());
            node.metadata
                .securities
                .extend(metadata.securities.iter().cloned());
        }

        for filter in router.filters() {
            let info = format!("{filter:?}");
            if info.starts_with("path:") {
                let path = info
                    .split_once(':')
                    .expect("split once by ':' should not be get `None`")
                    .1;
                node.path = Some(normalize_oapi_path(path));
            } else if info.starts_with("method:") {
                match info
                    .split_once(':')
                    .expect("split once by ':' should not be get `None`.")
                    .1
                {
                    "GET" => node.method = Some(PathItemType::Get),
                    "POST" => node.method = Some(PathItemType::Post),
                    "PUT" => node.method = Some(PathItemType::Put),
                    "DELETE" => node.method = Some(PathItemType::Delete),
                    "HEAD" => node.method = Some(PathItemType::Head),
                    "OPTIONS" => node.method = Some(PathItemType::Options),
                    "CONNECT" => node.method = Some(PathItemType::Connect),
                    "TRACE" => node.method = Some(PathItemType::Trace),
                    "PATCH" => node.method = Some(PathItemType::Patch),
                    _ => {}
                }
            }
        }
        node.handler_type_id = router.goal.as_ref().map(|h| h.type_id());
        node.handler_type_name = router.goal.as_ref().map(|h| h.type_name());
        let routers = router.routers();
        if !routers.is_empty() {
            for router in routers {
                node.children.push(Self::new(router, node.metadata.clone()));
            }
        }
        node
    }
}

/// A component for save router metadata.
type MetadataMap = RwLock<HashMap<usize, Metadata>>;
static METADATA_REGISTRY: LazyLock<MetadataMap> = LazyLock::new(MetadataMap::default);

/// Router extension trait for openapi metadata.
pub trait RouterExt {
    /// Add security requirement to the router.
    ///
    /// All endpoints in the router and it's descents will inherit this security requirement.
    #[must_use]
    fn oapi_security(self, security: SecurityRequirement) -> Self;

    /// Add security requirements to the router.
    ///
    /// All endpoints in the router and it's descents will inherit these security requirements.
    #[must_use]
    fn oapi_securities<I>(self, security: I) -> Self
    where
        I: IntoIterator<Item = SecurityRequirement>;

    /// Add tag to the router.
    ///
    /// All endpoints in the router and it's descents will inherit this tag.
    #[must_use]
    fn oapi_tag(self, tag: impl Into<String>) -> Self;

    /// Add tags to the router.
    ///
    /// All endpoints in the router and it's descents will inherit these tags.
    #[must_use]
    fn oapi_tags<I, V>(self, tags: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<String>;
}

impl RouterExt for Router {
    fn oapi_security(self, security: SecurityRequirement) -> Self {
        let mut guard = METADATA_REGISTRY
            .write()
            .expect("failed to lock METADATA_REGISTRY for write");
        let metadata = guard.entry(self.id).or_default();
        metadata.securities.push(security);
        self
    }
    fn oapi_securities<I>(self, iter: I) -> Self
    where
        I: IntoIterator<Item = SecurityRequirement>,
    {
        let mut guard = METADATA_REGISTRY
            .write()
            .expect("failed to lock METADATA_REGISTRY for write");
        let metadata = guard.entry(self.id).or_default();
        metadata.securities.extend(iter);
        self
    }
    fn oapi_tag(self, tag: impl Into<String>) -> Self {
        let mut guard = METADATA_REGISTRY
            .write()
            .expect("failed to lock METADATA_REGISTRY for write");
        let metadata = guard.entry(self.id).or_default();
        metadata.tags.insert(tag.into());
        self
    }
    fn oapi_tags<I, V>(self, iter: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<String>,
    {
        let mut guard = METADATA_REGISTRY
            .write()
            .expect("failed to lock METADATA_REGISTRY for write");
        let metadata = guard.entry(self.id).or_default();
        metadata.tags.extend(iter.into_iter().map(Into::into));
        self
    }
}

#[non_exhaustive]
#[derive(Default, Clone, Debug)]
pub(crate) struct Metadata {
    pub(crate) tags: BTreeSet<String>,
    pub(crate) securities: Vec<SecurityRequirement>,
}

#[cfg(test)]
mod tests {
    use super::normalize_oapi_path;

    #[test]
    fn normalize_braced_path_constraints() {
        assert_eq!(normalize_oapi_path("/posts/{id}"), "/posts/{id}");
        assert_eq!(normalize_oapi_path("/posts/{id:num}"), "/posts/{id}");
        assert_eq!(
            normalize_oapi_path("/posts/{id:num(3..=10)}"),
            "/posts/{id}"
        );
        assert_eq!(normalize_oapi_path(r"/posts/{id|\d+}"), "/posts/{id}");
        assert_eq!(normalize_oapi_path("/posts/{id|[a-z]{2}}"), "/posts/{id}");
        assert_eq!(
            normalize_oapi_path("/posts/article_{id:num}"),
            "/posts/article_{id}"
        );
    }
}
