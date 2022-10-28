# Quick Start

## Installing Rust

If you don’t have Rust yet, we recommend you use ```rustup``` to manage your Rust installation. The [official rust guide](https://doc.rust-lang.org/book/ch01-01-installation.html) has a wonderful section on getting started.

Salvo currently has a minimum supported Rust version 1.59. Running ```rustup update``` will ensure you have the latest and greatest Rust version available. As such, this guide assumes you are running Rust 1.59 or later.

## Write first app

Create a new rust project:

```bash
cargo new hello_salvo --bin
```

Add this to `Cargo.toml`

```toml
[dependencies]
salvo = "*"
tokio = { version = "1", features = ["macros"] }
```

Create a simple function handler in the main.rs file, we call it `hello_world`, this function just render plain text ```"Hello world"```. In the ```main``` function, we need to create a root Router first, and then create a server and call it's ```bind``` function:

```rust
use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello world"
}
#[tokio::main]
async fn main() {
    let router = Router::new().get(hello_world);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```
Congratulations! Your first app has done! Just run ```cargo run``` to run this app.

## More about handler

There are many ways to write function handler.

- The orginal format is:

    ```rust
    #[handler]
    async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        res.render("Hello world");
    }
    ```

- You can omit function arguments if they are not used, like ```_req```, ```_depot```, ```_ctrl``` in this example:

    ``` rust
    #[handler]
    async fn hello_world(res: &mut Response) {
        res.render("Hello world");
    }
    ```

- Any type can be function handler's return value if it implements ```Writer```. For example ```&str``` implements ```Writer``` and it will render string as plain text:

    ```rust
    #[handler]
    async fn hello_world(res: &mut Response) -> &'static str {// just return &str
        "Hello world"
    }
    ```

- The more common situation is we want to return a ```Result<T, E>``` to implify error handling. If ```T``` and ```E``` implements ```Writer```, ```Result<T, E>``` can be function handler's return type:
  
    ```rust
    #[handler]
    async fn hello_world(res: &mut Response) -> Result<&'static str, ()> {// return Result
        Ok("Hello world")
    }
    ```

## Run more examples
The absolute fastest way to start experimenting with Salvo is to clone the
Salvo repository and run the included examples in the `examples/` directory.
For instance, the following set of commands runs the `hello_world` example:

```sh
git clone https://github.com/salvo-rs/salvo.git
cd salvo
cargo run --bin example-hello_world
```

There are numerous examples in the `examples/` directory. They can all be run
with `cargo run --bin example-<name>`.