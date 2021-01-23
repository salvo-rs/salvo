+++
title = "路由系统"
weight = 1
sort_by = "weight"
template = "page.html"
+++

Salvo 采用树状的路由系统. 树状的路由系统, 可以方便地写出复杂的路由结构. 比如:

```rust
 use salvo::prelude::*;

#[tokio::main]
async fn main() {
    let debug_mode = true;
    let admin_mode = true;
    let router = Router::new()
        .get(handle)
        .push(
            Router::new()
                .path("users")
                .before(auth)
                .post(handle)
                .push(Router::new().path(r"<id:/\d+/>").post(handle).delete(handle)),
        )
        .push(
            Router::new()
                .path("users")
                .get(handle)
                .push(Router::new().path(r"<id:/\d+/>").get(handle)),
        ).push_when(|_|if debug_mode {
            Some(Router::new().path("debug").get(handle))
        } else {
            None
        }).visit(|parent|{
            if admin_mode {
                parent.push(Router::new().path("admin").get(handle))
            } else {
                parent
            }
        });

    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}

#[fn_handler]
async fn handle(res: &mut Response) {
    res.handle("Fake handle");
}
#[fn_handler]
async fn auth(res: &mut Response) {
    res.handle("Fake auth handle");
}
```

整个代码并不需要声明过多的变量来绑定不同的 Router, 只需要按层级将多个 Routers 组合到一起. 即使是于是了有逻辑判断的情况, 也是可以通过 ```visit``` 等函数方便地写出链式风格的代码.

对于每一个 Router, 可以通过 before 和 after 添加多个中间件, 中间件其实跟普通的 Handler 也是一样的. 在父级 Router 上添加的中间件, 会影响父级本身以及所有子孙的 Routers.

对于上面的例子, 修改删除用户需要登录, 于是在 ```users``` 路径上添加的 auth 的中间件:

```rust
Router::new()
    .path("users")
    .before(auth)
    .post(handle)
    .push(Router::new().path(r"<id:/\d+/>").post(handle).delete(handle))
```

然而对于 ```http://localhost:7878/users``` 下所有的子路径并不需要都登录授权, 显示用户列表和用户信息就不需要验证. 这种情况下, 我们可以在添加一个具体相同路径 (```users```) 的 Router, 但是不添加 ```auth``` 中间件:

```rust
Router::new()
    .path("users")
    .get(handle)
    .push(Router::new().path(r"<id:/\d+/>").get(handle))
```