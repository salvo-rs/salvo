# NamedFile

Salvo 提供了 ```salvo::fs::NamedFile```, 可以用它高效地發送文件到客戶端. 它並不會把文件都加載入緩存, 而是根據請求的 `Range` 加載部分內容發送至客戶端.

## 示例代碼

```rust
#[handler]
async fn send_file(req: &mut Request, res: &mut Response) {
    NamedFile::send_file("/file/to/path", req, res).await;
}
```

