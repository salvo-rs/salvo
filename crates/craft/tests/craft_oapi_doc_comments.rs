#![allow(missing_docs)]

use std::sync::Arc;

use salvo::oapi::extract::QueryParam;
use salvo::oapi::{OpenApi, PathItemType};
use salvo::prelude::*;
use salvo_craft::craft;

#[derive(Clone, Debug)]
pub struct DocService {
    base: i64,
}

#[craft]
impl DocService {
    /// Add with self
    ///
    /// Adds the base to the two query parameters.
    #[craft(endpoint)]
    fn add_ref(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.base + *left + *right).to_string()
    }

    /// Add with arc
    ///
    /// Same as `add_ref` but takes `Arc<Self>`.
    // `#[doc(alias = ...)]` is valid on methods but not on impl blocks. The macro
    // must not forward list-form `doc` attributes to the generated impl block.
    #[doc(alias = "add-arc")]
    #[craft(endpoint)]
    fn add_arc(self: Arc<Self>, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.base + *left + *right).to_string()
    }

    /// Add plain
    ///
    /// Adds two query parameters without a receiver.
    #[craft(endpoint)]
    fn add_plain(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (*left + *right).to_string()
    }
}

fn description_for(doc: &OpenApi, path: &str) -> Option<String> {
    doc.paths
        .get(path)
        .and_then(|item| item.operations.get(&PathItemType::Get))
        .and_then(|op| op.description.clone())
}

fn summary_for(doc: &OpenApi, path: &str) -> Option<String> {
    doc.paths
        .get(path)
        .and_then(|item| item.operations.get(&PathItemType::Get))
        .and_then(|op| op.summary.clone())
}

#[test]
fn craft_endpoint_propagates_doc_comments() {
    let service = Arc::new(DocService { base: 1 });
    let router = Router::new()
        .push(Router::with_path("add-ref").get(service.add_ref()))
        .push(Router::with_path("add-arc").get(service.add_arc()))
        .push(Router::with_path("add-plain").get(DocService::add_plain()));

    let doc = OpenApi::new("Craft Doc Test", "0.0.1").merge_router(&router);

    // `&self` receiver: doc comments must reach the OpenAPI operation.
    assert_eq!(
        summary_for(&doc, "/add-ref").as_deref(),
        Some("Add with self")
    );
    assert_eq!(
        description_for(&doc, "/add-ref").as_deref(),
        Some("Adds the base to the two query parameters.")
    );

    // `Arc<Self>` receiver: same expectation.
    assert_eq!(
        summary_for(&doc, "/add-arc").as_deref(),
        Some("Add with arc")
    );
    assert_eq!(
        description_for(&doc, "/add-arc").as_deref(),
        Some("Same as `add_ref` but takes `Arc<Self>`.")
    );

    // No-receiver: already worked, but assert it stays correct.
    assert_eq!(
        summary_for(&doc, "/add-plain").as_deref(),
        Some("Add plain")
    );
    assert_eq!(
        description_for(&doc, "/add-plain").as_deref(),
        Some("Adds two query parameters without a receiver.")
    );
}
