Salvo is a simple web framework written by rust. It is simple to use it to build website, REST API.


## Quick start
You can view samples [here](https://github.com/kenorld/salvo/tree/master/examples) or read docs [here](https://docs.rs/salvo/0.1.6/salvo/).

Here's an example of a Salvo application:

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
    resp.render_plain_text("Hello World");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut router = Router::new("/");
    router.get(hello_world);
    let server = Server::new(router);
    server.serve().await?;
    Ok(())
}
```

## License

Salvo is licensed under MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)