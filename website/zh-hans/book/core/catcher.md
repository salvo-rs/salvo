# Catcher

```Catcher``` 是用于处理页面返回 HTTP 状态码为错误的情况下, 如何显示页面的抽象.

```rust
pub trait Catcher: Send + Sync + 'static {
    fn catch(&self, req: &Request, res: &mut Response) -> bool;
}
```

一个网站应用可以指定多个不同的 Catcher 对象处理错误. 它们被保存在 Service 的字段中:

```rust
pub struct Service {
    pub(crate) router: Arc<Router>,
    pub(crate) catchers: Arc<Vec<Box<dyn Catcher>>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}
```

可以通过 ```Server``` 的 ```with_catchers``` 函数设置它们:

```rust
struct Handle404;
impl Catcher for Handle404 {
    fn catch(&self, _req: &Request, _depot: &Depot, res: &mut Response) -> bool {
        if let Some(StatusCode::NOT_FOUND) = res.status_code() {
            res.render("Custom 404 Error Page");
            true
        } else {
            false
        }
    }
}
#[tokio::main]
async fn main() {
    let router = Router::new().get(hello_world);
    let catchers: Vec<Box<dyn Catcher>> = vec![Box::new(Handle404)];
    let service = Service::new(router).with_catchers(catchers);
    Server::new(TcpListener::new("0.0.0.0:7878"))
        .serve(service())
        .await;
}
```

当网站请求结果有错误时, 首先试图通过用户自己设置的 ```Catcher``` 设置错误页面, 如果 ```Catcher``` 捕获错误, 则返回 ```true```. 

如果您自己设置的 ```Catcher``` 都没有捕获这个错误, 则系统使用默认的 ```Catcher``` 实现 ```CatcherImpl``` 捕获处理错误, 发送默认的错误页面. 默认的错误实现 ```CatcherImpl``` 支持以 ```XML```, ```JSON```, ```HTML```, ```Text``` 格式发送错误页面.