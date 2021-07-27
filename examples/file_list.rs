use salvo_core::prelude::*;
use salvo_extra::serve::{Options, StaticDir};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().path("<**path>").get(StaticDir::width_options(
        vec!["examples/static/body", "examples/static/girl"],
        Options {
            dot_files: false,
            listing: true,
            defaults: vec!["index.html".to_owned()],
        },
    ));
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
