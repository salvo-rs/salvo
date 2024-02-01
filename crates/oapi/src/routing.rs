use std::any::TypeId;
use std::collections::{BTreeSet, HashMap};
use std::sync::MutexGuard;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use regex::Regex;
use salvo_core::Router;

use crate::{path::PathItemType, security, SecurityRequirement};

#[derive(Debug, Default)]
pub(crate) struct NormNode {
    pub(crate) handler_type_id: Option<TypeId>,
    pub(crate) handler_type_name: Option<&'static str>,
    pub(crate) method: Option<PathItemType>,
    pub(crate) path: Option<String>,
    pub(crate) children: Vec<NormNode>,
}

impl NormNode {
    pub(crate) fn new(router: &Router) -> Self {
        let mut node = NormNode::default();
        let regex = Regex::new(r#"<([^/:>]+)(:[^>]*)?>"#).unwrap();
        for filter in router.filters() {
            let info = format!("{filter:?}");
            if info.starts_with("path:") {
                let path = info.split_once(':').unwrap().1;
                node.path = Some(regex.replace_all(path, "{$1}").to_string());
            } else if info.starts_with("method:") {
                match info.split_once(':').unwrap().1 {
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
                node.children.push(NormNode::new(router));
            }
        }
        node
    }
}

/// A component for save router metadata.
type MetadataMap = RwLock<HashMap<usize, Metadata>>;
static METADATA_REGISTRY: Lazy<MetadataMap> = Lazy::new(MetadataMap::default);

pub trait RouterExt {
    fn oapi_security(self, security: SecurityRequirement) -> Self;
    fn oapi_tag(self, tag: impl Into<String>) -> Self;
}

impl RouterExt for Router {
    fn oapi_security(self, security: SecurityRequirement) -> Self {
        let mut guard = METADATA_REGISTRY
            .write()
            .expect("failed to lock METADATA_REGISTRY for write");
        let metadata = guard.entry(self.id).or_insert_with(|| Metadata::default());
        metadata.securities.push(security);
        self
    }
    fn oapi_tag(self, tag: impl Into<String>) -> Self {
        let mut guard = METADATA_REGISTRY
            .write()
            .expect("failed to lock METADATA_REGISTRY for write");
        let metadata = guard.entry(self.id).or_insert_with(|| Metadata::default());
        metadata.tags.insert(tag.into());
        self
    }
}

#[non_exhaustive]
#[derive(Default)]
pub(crate) struct Metadata {
    pub(crate) securities: Vec<SecurityRequirement>,
    pub(crate) tags: BTreeSet<String>,
}
