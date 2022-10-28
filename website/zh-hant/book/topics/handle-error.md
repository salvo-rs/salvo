# 錯誤處理

## Rust 應用中的常規錯誤處理方式

Rust 的錯誤處理不同於 Java 等語言, 它沒有 `try...catch` 這種玩意, 正常的做法是在應用程序層面定義全局的錯誤處理類型:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("io: `{0}`")]
    Io(#[from] io::Error),
    #[error("utf8: `{0}`")]
    FromUtf8(#[from] FromUtf8Error),
    #[error("diesel: `{0}`")]
    Diesel(#[from] diesel::result::Error),
    ...
}

pub type AppResult<T> = Result<T, AppError>;
```

這裏使用了 `thiserror` 這個庫, 它可以方便地定義你自己的自定義錯誤類型, 簡化代碼. 為了簡單書寫, 順便定義一個 `AppResult`.


## Handler 中的錯誤處理

在 Salvo 中, `Handler` 也經常會遇到各式錯誤, 比如: 數據庫連接錯誤, 文件訪問錯誤, 網絡連接錯誤等等. 對於這個類型的錯誤, 可以采用上述的錯誤處理手法:

```rust
#[handler]
async fn home()-> AppResult<()> {

}
```

這裏的 `home` 就直接返回了一個 `AppResult<()>`. 但是, 這個錯誤改如何顯示呢? 我們需要為 `AppResult` 這個自定義錯誤類型實現 `Writer`, 在這個實現中我們可以決定如何顯示錯誤:

```rust
#[async_trait]
impl Writer for AppError {
    async fn write(mut self, _req: &mut Request, depot: &mut Depot, res: &mut Response) {
        res.render(Text::Plain("I'm a error, hahaha!"));
    }
}
```

`Errror` 中往往包含一些敏感信息, 一般情況下, 並不想被普通用戶看到, 那樣也太不安全了, 一點點隱私也沒有了. 但是, 如果你是開發人員或者網站管理員, 或許想法就不一樣了, 你希望錯誤能把外衣脫得光光的, 讓你看到最真實的錯誤信息.

可以看到, `write` 的方法中, 我們其實是可以拿到 `Request` 和 `Depot` 的引用的, 這就可以很方便地實現上面的騷操作了:

```rust
#[async_trait]
impl Writer for AppError {
    async fn write(mut self, _req: &mut Request, depot: &mut Depot, res: &mut Response) {
        let user = depot.obtain::<User>();
        if user.is_admin {
            res.render(Text::Plain(e.to_string()));
        } else {
            res.render(Text::Plain("I'm a error, hahaha!"));
        }
    }
}
```

## 錯誤頁面的顯示

Salvo 中自帶的錯誤頁面在絕大部分情況下是滿足需求的, 它可以根據請求的數據類型, 顯示 Html, Json 或者 Xml 頁面. 然而, 某些情況下, 我們依然期望自定義錯誤頁面的顯示.

這個可以通過自定義 `Catcher` 實現. 詳細的介紹可以查看 [`Catcher`](../core/catcher/) 部分的講解.