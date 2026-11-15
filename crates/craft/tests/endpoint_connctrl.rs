//! Endpoint connection injection has the same shared state as ordinary handlers.
use salvo::oapi::extract::QueryParam;
use salvo::oapi::{OpenApi, endpoint};
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[endpoint]
fn abort(conn: &mut ConnCtrl, ctrl: &mut FlowCtrl) -> &'static str {
    conn.abort();
    assert!(ctrl.conn().is_aborted());
    "aborted"
}

#[endpoint]
async fn relax(conn: &ConnCtrl) {
    conn.relax_timeouts();
}

struct Graceful;
#[endpoint]
impl Graceful {
    async fn handle(&self, conn: &mut salvo::ConnCtrl) -> &'static str {
        conn.graceful_shutdown();
        "graceful"
    }
}

#[derive(Clone)]
struct Service;
#[craft]
impl Service {
    #[craft(endpoint)]
    fn mutate(&self, conn: &mut ConnCtrl) {
        conn.abort();
    }
}

#[endpoint]
async fn validated(_value: QueryParam<u32>, conn: &mut ConnCtrl) -> &'static str {
    conn.abort();
    "valid"
}

async fn run(handler: impl Handler, query: &str) -> (FlowCtrl, Response) {
    let mut req = TestClient::get(format!("http://localhost/{query}")).build();
    let mut depot = Depot::new();
    let mut res = Response::new();
    let mut ctrl = FlowCtrl::default();
    handler
        .handle(&mut req, &mut depot, &mut res, &mut ctrl)
        .await;
    (ctrl, res)
}

#[tokio::test]
async fn function_impl_and_crafted_endpoints_share_flow_connection_state() {
    let (ctrl, mut res) = run(abort, "").await;
    assert!(ctrl.conn().is_aborted());
    assert_eq!(res.take_string().await.unwrap(), "aborted");
    let (ctrl, _) = run(relax, "").await;
    assert!(ctrl.conn().is_relaxed());
    let (ctrl, mut res) = run(Graceful, "").await;
    assert!(ctrl.conn().is_graceful_shutdown());
    assert_eq!(res.take_string().await.unwrap(), "graceful");
    let (ctrl, _) = run(Service.mutate(), "").await;
    assert!(ctrl.conn().is_aborted());
}

#[tokio::test]
async fn failed_extraction_does_not_invoke_endpoint_or_document_connection_parameters() {
    let (ctrl, res) = run(validated, "?value=invalid").await;
    assert!(!ctrl.conn().is_aborted());
    assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));
    let (ctrl, _) = run(validated, "?value=7").await;
    assert!(ctrl.conn().is_aborted());

    let router = Router::new()
        .push(Router::with_path("abort").get(abort))
        .push(Router::with_path("validated").get(validated));
    let doc = OpenApi::new("connections", "1").merge_router(&router);
    let operation = &doc.paths.get("/abort").unwrap().operations[&salvo::oapi::PathItemType::Get];
    assert!(operation.parameters.0.is_empty());
    let operation =
        &doc.paths.get("/validated").unwrap().operations[&salvo::oapi::PathItemType::Get];
    assert_eq!(operation.parameters.0.len(), 1);
}
