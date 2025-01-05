use regex::Regex;
use salvo::prelude::*;
use salvo::routing::PathFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    // invalid guid: 123e4567-h89b-12d3-a456-9AC7CBDCEE52
    // valid guid: 123e4567-e89b-12d3-a456-9AC7CBDCEE52
    PathFilter::register_wisp_regex(
        "guid",
        Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
    );

    let router = Router::with_path("{id:guid}").get(index);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[handler]
async fn index(req: &mut Request, res: &mut Response) {
    res.render(req.params().get::<str>("id").unwrap());
}
