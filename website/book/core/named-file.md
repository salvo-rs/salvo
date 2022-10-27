---
title: "Static File"
weight: 2060
menu:
  book:
    parent: "core"
---

Salvo provides ```salvo::fs::NamedFile```, which can be used to send files to clients efficiently:

```rust
#[handler]
async fn send_file(req: &mut Request, res: &mut Response) {
    NamedFile::send_file("/file/to/path", req, res).await;
}
```

