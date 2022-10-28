# Response

在 ```Handler``` 中, ```Response``` 會被作為參數傳入:

```rust
#[handler]
async fn hello_world(res: &mut Response) {
    res.render("Hello world!");
}
```

```Response``` 在服務器接收到客戶端請求後, 任何匹配到的 ```Handler``` 和中間件都可以向裏面寫入數據. 在某些情況下, 比如某個中間件希望阻止後續的中間件和 ```Handler``` 執行, 您可以使用 ```FlowCtrl```:

```rust
#[handler]
async fn hello_world(res: &mut Response, ctrl: &mut FlowCtrl) {
    ctrl.skip_rest();
    res.render("Hello world!");
}
```

## 寫入內容

向 ```Response``` 中寫入數據是非常簡單的:

- 寫入純文本數據

    ```rust
    res.render("Hello world!");
    ``` 

- 寫入 JSON 序列化數據
    
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

- 寫入 HTML
    
    ```rust
    res.render(Text::Html("<html><body>hello</body></html>"));
    ```

## 寫入 HTTP 錯誤


- 使用 ```set_http_error``` 可以向 ```Response``` 寫入詳細錯誤信息.

    ```rust
    use salvo::http::errors::*;
    res.set_http_error(StatusError::internal_server_error().with_summary("error when serialize object to json"))
    ```

- 如果您不需要自定義錯誤信息, 可以直接調用 ```set_http_code```.

    ```rust
    use salvo::http::StatusCode;
    res.set_status_code(StatusCode::BAD_REQUEST);
    ```