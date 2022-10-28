# Depot

Depot 是用于保存一次请求中涉及到的临时数据. 中间件可以将自己处理的临时数据放入 Depot, 供后续程序使用.

当一个服务器接收到一个客户浏览器发来的请求后会创建一个 ```Depot``` 的实例. 这个实例会在所有的中间件和 ```Handler``` 处理完请求后被销毁.

比如说, 我们可以在登录的中间件中设置 ```current_user```, 然后在后续的中间件或者 ```Handler``` 中读取当前用户信息.

```rust
use salvo::prelude::*;

#[handler]
async fn set_user(depot: &mut Depot)  {
  depot.insert("current_user", "Elon Musk");
}
#[handler]
async fn home(depot: &mut Depot) -> String  {
  // 需要注意的是, 这里的类型必须是 &str, 而不是 String, 因为当初存入的数据类型为 &str.
  let user = depot.get::<&str>("current_user").copied().unwrap();
  format!("Hey {}, I love your money and girls!", user)
}

#[tokio::main]
async fn main() {
    let router = Router::with_hoop(set_user).get(home);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```

## 通过 `insert` 和 `get` 设置和取出数据

 正如上面所示, 可以通过 `insert` 把 `key` 和 `value` 插入到 `Depot` 中. 对于这一类型的值, 直接用 `get` 取出.

```rust
depot.insert("a", "b");
assert_eq!(depot.get::<&str>("a").copied().unwrap(), "b")
```

 如果不存在这个 `key`, 或者 `key` 存在, 但是类型不匹配, 则返回 `None`.

## 通过 `inject` 和 `obtain` 设置和取出数据

有时, 存在一些不需要关系具体 `key`, 对于这种类型也存在唯一实例的情况. 可以使用 `inject` 插入数据, 然后使用 `obtain` 取出数据. 它们不需要你提供 `key`.

```rust
depot.inject(Config::new());
depot.obtain::<Config>();
```