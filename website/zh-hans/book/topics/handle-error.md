# 错误处理

## Rust 应用中的常规错误处理方式

Rust 的错误处理不同于 Java 等语言, 它没有 `try...catch` 这种玩意, 正常的做法是在应用程序层面定义全局的错误处理类型:

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

这里使用了 `thiserror` 这个库, 它可以方便地定义你自己的自定义错误类型, 简化代码. 为了简单书写, 顺便定义一个 `AppResult`.


## Handler 中的错误处理

在 Salvo 中, `Handler` 也经常会遇到各式错误, 比如: 数据库连接错误, 文件访问错误, 网络连接错误等等. 对于这个类型的错误, 可以采用上述的错误处理手法:

```rust
#[handler]
async fn home()-> AppResult<()> {

}
```

这里的 `home` 就直接返回了一个 `AppResult<()>`. 但是, 这个错误改如何显示呢? 我们需要为 `AppResult` 这个自定义错误类型实现 `Writer`, 在这个实现中我们可以决定如何显示错误:

```rust
#[async_trait]
impl Writer for AppError {
    async fn write(mut self, _req: &mut Request, depot: &mut Depot, res: &mut Response) {
        res.render(Text::Plain("I'm a error, hahaha!"));
    }
}
```

`Errror` 中往往包含一些敏感信息, 一般情况下, 并不想被普通用户看到, 那样也太不安全了, 一点点隐私也没有了. 但是, 如果你是开发人员或者网站管理员, 或许想法就不一样了, 你希望错误能把外衣脱得光光的, 让你看到最真实的错误信息.

可以看到, `write` 的方法中, 我们其实是可以拿到 `Request` 和 `Depot` 的引用的, 这就可以很方便地实现上面的骚操作了:

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

## 错误页面的显示

Salvo 中自带的错误页面在绝大部分情况下是满足需求的, 它可以根据请求的数据类型, 显示 Html, Json 或者 Xml 页面. 然而, 某些情况下, 我们依然期望自定义错误页面的显示.

这个可以通过自定义 `Catcher` 实现. 详细的介绍可以查看 [`Catcher`](../core/catcher/) 部分的讲解.