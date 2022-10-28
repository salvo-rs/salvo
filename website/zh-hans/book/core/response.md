# Response

在 ```Handler``` 中, ```Response``` 会被作为参数传入:

```rust
#[handler]
async fn hello_world(res: &mut Response) {
    res.render("Hello world!");
}
```

```Response``` 在服务器接收到客户端请求后, 任何匹配到的 ```Handler``` 和中间件都可以向里面写入数据. 在某些情况下, 比如某个中间件希望阻止后续的中间件和 ```Handler``` 执行, 您可以使用 ```FlowCtrl```:

```rust
#[handler]
async fn hello_world(res: &mut Response, ctrl: &mut FlowCtrl) {
    ctrl.skip_rest();
    res.render("Hello world!");
}
```

## 写入内容

向 ```Response``` 中写入数据是非常简单的:

- 写入纯文本数据

    ```rust
    res.render("Hello world!");
    ``` 

- 写入 JSON 序列化数据
    
    ```rust
    use serde::Serialize;
    use salvo::prelude::Json;

    #[derive(Serialize, Debug)]
    struct User {
        name: String,
    }
    let user = User{name: "jobs"};
    res.render(Json(user));
    ```

- 写入 HTML
    
    ```rust
    res.render(Text::Html("<html><body>hello</body></html>"));
    ```

## 写入 HTTP 错误


- 使用 ```set_http_error``` 可以向 ```Response``` 写入详细错误信息.

    ```rust
    use salvo::http::errors::*;
    res.set_http_error(StatusError::internal_server_error().with_summary("error when serialize object to json"))
    ```

- 如果您不需要自定义错误信息, 可以直接调用 ```set_http_code```.

    ```rust
    use salvo::http::StatusCode;
    res.set_status_code(StatusCode::BAD_REQUEST);
    ```