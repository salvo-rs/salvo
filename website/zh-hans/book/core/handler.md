# Handler

## 什么是 Handler

Handler 是负责负责处理 Request 请求的具体对象.  Hander 本身是一个 Trait, 内部包含一个 ```handle``` 的异步方法:

```rust
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}
```

## 函数式 Handler

很多时候只是希望通过函数作为 ```Handler``` 处理请求. 可以添加 `Handler` 将普通函数转为 ```Handler```.

处理函数默认签名包含四个参数, 依次是, ```&mut Request, &mut Depot. &mut Response, &mut FlowCtrl```. Depot 是一个临时存储, 可以存储本次请求相关的数据. 

中间件其实也是 ```Handler```, 它们可以对请求到达正式处理请求的 ```Handler``` 之前或者之后作一些处理 比如：登录验证, 数据压缩等.

中间件是通过 ```Router``` 的 ```hoop``` 函数添加的. 被添加的中间件会影响当前的 ```Router``` 和它内部所有子孙 ```Router```.

正常项目中使用得最多的应该是 `Handler`, 它是一个 ```proc macro```, 加在函数上可以将函数转变成一个 ```Handler```:

```rust
#[handler]
async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render("Hello world");
}
```

某些参数如果不需要, 可以直接省略. 事实上, 这三个参数的顺序可以按喜好自由调整, 也可以省略任何一个或者多个参数. 下面这些写法都是可以的:

```rust
#[handler]
async fn hello_world(req: &mut Request, res: &mut Response) {
}
#[handler]
async fn hello_world(depot: &mut Depot) {
}
#[handler]
async fn hello_world(res: &mut Response) {
}
```

## 处理错误

Salvo 中的 `Handler` 可以返回 ```Result```, 只需要 ```Result``` 中的 ```Ok``` 和 ```Err``` 的类型都实现 ```Writer``` trait.
考虑到 anyow 的使用比较广泛, 在开启 ```anyhow``` 功能后, ```anyhow::Error``` 会实现 ```Writer``` trait. ```anyhow::Error``` 会被映射为 ```InternalServerError```. 

```rust
#[cfg(feature = "anyhow")]
#[async_trait]
impl Writer for ::anyhow::Error {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_http_error(StatusError::internal_server_error());
    }
}
```

对于自定义错误类型, 你可以根据需要输出不同的错误页面.

```rust
use salvo::anyhow;
use salvo::prelude::*;

struct CustomError;
#[async_trait]
impl Writer for CustomError {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render("custom error");
        res.set_http_error(StatusError::internal_server_error());
    }
}

#[handler]
async fn handle_anyhow() -> Result<(), anyhow::Error> {
    Err(anyhow::anyhow!("anyhow error"))
}
#[handler]
async fn handle_custom() -> Result<(), CustomError> {
    Err(CustomError)
}

#[tokio::main]
async fn main() {
    let router = Router::new()
        .push(Router::new().path("anyhow").get(handle_anyhow))
        .push(Router::new().path("custom").get(handle_custom));
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```

## 直接实现 Handler Trait

```rust
pub struct MaxSizeHandler(u64);
#[async_trait]
impl Handler for MaxSizeHandler {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(upper) = req.body().and_then(|body| body.size_hint().upper()) {
            if upper > self.0 {
                res.set_status_error(StatusError::payload_too_large());
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
    }
}
```

## `#[handler]` 的使用

`#[handler]` 可以大量简化代码的书写, 并且提升代码的灵活度. 它可以加在一个函数上, 让它实现 `Handler`:

```rust
#[handler]
async fn hello() -> &'static str {
    "hello world!"
}
```

这等价于:

```rust
struct hello;

#[async_trait]
impl Hander for hello {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {
        res.render(Text::Plain("hello world!"));
    }
}
```

可以看到, 在使用 `#[handler]` 的情况下, 代码变得简单很多:
- 不再需要手工添加 `#[async_trait]`.
- 函数中不需要的参数已经省略, 对于需要的参数也可以按任意顺序排布.
- 对于实现了 `Writer` 或者 `Piece` 抽象的对象, 可以直接作为函数的返回值. 在这里 `&'static str` 实现了 `Piece`, 于是可以直接作为函数返回值返回.

`#[handler]` 不仅可以加在函数上, 也可以加在 `struct` 的 `impl` 上:

```rust
struct Hello;

#[handler]
impl Hello {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {
        res.render(Text::Plain("hello world!"));
    }
}
```