# Serve Static

Middleware that provides static files or embedded files as services.

## Config Cargo.toml

```toml
salvo = { version = "*", features = ["serve-static"] }
```

## Main Features

* `StaticDir` provides support for static local folders. You can take a list of multiple folders as an argument. For example:

    ```rust
    use salvo::prelude::*;
    use salvo::serve_static::StaticDir;

    #[tokio::main]
    async fn main() {
        tracing_subscriber::fmt().init();

        let router = Router::with_path("<**path>").get(
            StaticDir::new([
                "examples/static-dir-list/static/boy",
                "examples/static-dir-list/static/girl",
            ])
            .with_defaults("index.html")
            .with_listing(true),
        );
        tracing::info!("Listening on http://127.0.0.1:7878");
        let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
    }
    ```
    If the corresponding file is not found in the first folder, it will look in the second folder.

* Provides support for `rust-embed`, such as:
    ```rust
    use rust_embed::RustEmbed;
    use salvo::prelude::*;
    use salvo::serve_static::static_embed;

    #[derive(RustEmbed)]
    #[folder = "static"]
    struct Assets;

    #[tokio::main]
    async fn main() {
        tracing_subscriber::fmt().init();

        let router = Router::with_path("<**path>").get(static_embed::<Assets>().with_fallback("index.html"));
        tracing::info!("Listening on http://127.0.0.1:7878");
        let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
    }
    ```

    `with_fallback` can be set to replace the file set here when the file is not found, which is useful for some single-page website applications.