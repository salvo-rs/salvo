# Writer

```Writer``` 用於寫入內容到 ```Response```:

```rust
#[async_trait]
pub trait Writer {
    async fn write(mut self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}
```

相比於 Handler:

```rust
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl);
}
```

他們直接的差別在於:
- 用途不一樣, ```Writer``` 代表將具體的內容寫入 ```Response```, 由具體的內容實現, 比如字符串, 錯誤信息等. 而 ```Handler``` 是用於處理整個請求的.
- ```Writer``` 是在 ```Handler``` 中被創建, 會在 ```write``` 函數調用時消耗自身, 是一次性調用. 而 ```Handler``` 是所有請求公用的;
- ```Writer``` 可以作為 ```Handler``` 的返回的 ```Result``` 中的內容;
- ```Writer``` 中不存在 ```FlowCtrl``` 參數, 無法控制整個請求的執行流程.

```Piece``` 實現了 ```Writer```, 但是比 ```Writer``` 能實現的功能更少:

```rust
pub trait Piece {
    fn render(self, res: &mut Response);
}
```

```Piece``` 的渲染函數只是寫入數據到 ```Response```中, 這個過程是無法從 ```Request``` 或者 ```Depot``` 中獲取信息的.