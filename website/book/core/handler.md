# Handler

## What is handler

Handler is the specific object responsible for processing Request requests. Handler is a Trait, which contains an asynchronous method of ```handle```: 

```rust
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}
```

## Function handler

In many cases, we just want to use functions as ```Handler``` to process requests. We can add `Handler` to convert ordinary functions to ```Handler```. The most commonly used in normal projects should be `Handler`, it is a ```proc macro```, adding to the function can turn the function into a ```Handler```: 

```rust
#[handler]
async fn hello_world(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    res.render("Hello world");
}
```

The default signature of the processing function contains four parameters, followed by ```&mut Request, &mut Depot, &mut Response, &mut FlowCtrl```. Depot is a temporary storage that can store data related to this request.

Middleware is actually a ```Handler```, they can do some processing before or after the request arrives at the ```Handler``` that officially processes the request, such as login verification, data compression, etc.

Middlewares is added through the ```hoop``` function of the ```Router```. The added middleware will affect the current ```Router``` and its internal all descendants of ```Router```.

If some parameters are not needed, they can be omitted directly. In fact, the order of these three parameters can be adjusted freely according to your preference, or any one or more parameters can be omitted. The following writing methods are all possible:

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

## Handle errors

`Handler` in Salvo can return ```Result```, only the types of ```Ok``` and ```Err``` in ```Result``` are implemented ```Writer``` trait. 
Taking into account the widespread use of ```anyhow```, the ```Writer``` implementation of ```anyhow::Error``` is provided by default, and ```anyhow::Error``` is Mapped to ```InternalServerError```. 

```rust
#[cfg(feature = "anyhow")]
#[async_trait]
impl Writer for ::anyhow::Error {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_http_error(StatusError::internal_server_error());
    }
}
```

For custom error types, you can output different error pages according to your needs. 

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

## Implement Handler trait directly

Under certain circumstances, We need to implment ```Handler``` direclty.

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

## `#[handler]` usage

`#[handler]` can greatly simplify the writing of the code, and improve the flexibility of the code. It can be added to a function to make it implement `Handler`:

```rust
#[handler]
async fn hello() -> &'static str {
    "hello world!"
}
````

This is equivalent to:

```rust
struct hello;

#[async_trait]
impl Handler for hello {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {
        res.render(Text::Plain("hello world!"));
    }
}
````

As you can see, in the case of using `#[handler]`, the code becomes much simpler:
- No need to manually add `#[async_trait]`.
- The parameters that are not needed in the function have been omitted, and the required parameters can be arranged in any order.
- For objects that implement `Writer` or `Piece` abstraction, it can be directly used as the return value of the function. Here `&'static str` implements `Piece`, so it can be returned directly as the return value of the function.

`#[handler]` can be added not only to functions, but also to `impl` of `struct`:

```rust
struct Hello;

#[handler]
impl Hello {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {
        res.render(Text::Plain("hello world!"));
    }
}
````