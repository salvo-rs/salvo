use std::any::TypeId;
use std::collections::{BTreeSet, HashMap};
use std::sync::{LazyLock, RwLock};

use regex::Regex;
use salvo_core::Router;

use crate::{SecurityRequirement, path::PathItemType};

#[derive(Debug, Default)]
pub(crate) struct NormNode {
    // pub(crate) router_id: usize,
    pub(crate) handler_type_id: Option<TypeId>,
    pub(crate) handler_type_name: Option<&'static str>,
    pub(crate) method: Option<PathItemType>,
    pub(crate) path: Option<String>,
    pub(crate) children: Vec<NormNode>,
    pub(crate) metadata: Metadata,
}

impl NormNode {
    pub(crate) fn new(router: &Router, inherted_metadata: Metadata) -> Self {
        let mut node = NormNode {
            // router_id: router.id,
            metadata: inherted_metadata,
            ..NormNode::default()
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

        let regex = Regex::new(r#"<([^/:>]+)(:[^>]*)?>"#).expect("invalid regex");
        for filter in router.filters() {
            let info = format!("{filter:?}");
            if info.starts_with("path:") {
                let path = info
                    .split_once(':')
                    .expect("split once by ':' should not be get `None`")
                    .1;
                node.path = Some(regex.replace_all(path, "{$1}").to_string());
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
                node.children
                    .push(NormNode::new(router, node.metadata.clone()));
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
    fn oapi_security(self, security: SecurityRequirement) -> Self;

    /// Add security requirements to the router.
    ///
    /// All endpoints in the router and it's descents will inherit these security requirements.
    fn oapi_securities<I>(self, security: I) -> Self
    where
        I: IntoIterator<Item = SecurityRequirement>;

    /// Add tag to the router.
    ///
    /// All endpoints in the router and it's descents will inherit this tag.
    fn oapi_tag(self, tag: impl Into<String>) -> Self;

    /// Add tags to the router.
    ///
    /// All endpoints in the router and it's descents will inherit thes tags.
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
