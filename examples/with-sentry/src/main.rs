use salvo::prelude::*;
use tracing_subscriber::prelude::*;

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(sentry_tracing::layer())
        .init();
    let _sentry;

    if let Ok(sentry_dsn) = std::env::var("SENTRY_DSN") {
        _sentry = sentry::init((
            sentry_dsn,
            sentry::ClientOptions {
                release: sentry::release_name!(),
                traces_sample_rate: 1.0,
                ..Default::default()
            },
        ));
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(http());
}

async fn http() {
    let router = Router::new()
        .hoop(sentry_tower::NewSentryLayer::new_from_top().compat())
        .hoop(sentry_tower::SentryHttpLayer::with_transaction().compat());
    let acceptor = TcpListener::new("0.0.0.0:8080").bind().await;
    Server::new(acceptor).serve(router).await;
}
