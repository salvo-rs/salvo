use salvo_core::prelude::*;
use salvo_extra::serve::{Options, StaticDir};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(StaticDir::width_options(
        vec!["examples/static/boy", "examples/static/girl"],
        Options {
            dot_files: false,
            listing: true,
            defaults: vec!["index.html".to_owned()],
        },
    ));
    Server::bind(&"127.0.0.1:7878".parse().unwrap()).serve(Service::new(router)).await.unwrap();
}
