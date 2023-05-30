use std::any::TypeId;

use regex::Regex;
use salvo_core::Router;

use crate::path::PathItemType;

#[derive(Debug, Default)]
pub(crate) struct NormNode {
    pub(crate) type_id: Option<TypeId>,
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
        node.type_id = router.handler.as_ref().map(|h| h.type_id());
        let routers = router.routers();
        if !routers.is_empty() {
            for router in routers {
                node.children.push(NormNode::new(router));
            }
        }
        node
    }
}
