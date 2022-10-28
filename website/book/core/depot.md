# Depot

Depot is used to save data when process current request. It is useful for middlewares to share data.

A depot instance created when server get a request from client. The depot will dropped when all process for this request done.

For example, we can set ```current_user``` in ```set_user```, and then use this value in the following middlewares and handlers.

```rust
use salvo::prelude::*;

#[handler]
async fn set_user(depot: &mut Depot)  {
  depot.insert("current_user", "Elon Musk");
}
#[handler]
async fn home(depot: &mut Depot) -> String  {
  // Notic: Don't use String here, because you inserted a &str.
  let user = depot.get::<&str>("current_user").copied().unwrap();
  format!("Hey {}, I love your money and girls!", user)
}

#[tokio::main]
async fn main() {
    let router = Router::with_hoop(set_user).get(home);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```

## Set and retrieve data via `insert` and `get`

  As shown above, `key` and `value` can be inserted into `Depot` via `insert`. For values of this type, `get` can be used to retrieve them directly.

```rust
depot.insert("a", "b");
assert_eq!(depot.get::<&str>("a").copied().unwrap(), "b")
````

  Returns `None` if the `key` does not exist, or if the `key` exists, but the types do not match.

## Set and retrieve data via `inject` and `obtain`

Sometimes, there are cases where you don't need a relation-specific `key`, and there is also a unique instance of that type. You can use `inject` to inject data, and `obtain` to get data out. They don't require you to provide a `key`.

```rust
depot.inject(Config::new());
depot.obtain::<Config>();
````