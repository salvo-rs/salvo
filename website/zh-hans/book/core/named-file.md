# NamedFile

Salvo 提供了 ```salvo::fs::NamedFile```, 可以用它高效地发送文件到客户端. 它并不会把文件都加载入缓存, 而是根据请求的 `Range` 加载部分内容发送至客户端.

## 示例代码

```rust
#[handler]
async fn send_file(req: &mut Request, res: &mut Response) {
    NamedFile::send_file("/file/to/path", req, res).await;
}
```

