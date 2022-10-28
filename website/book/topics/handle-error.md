# Handle Error

## General error handling in Rust applications

Rust's error handling is different from languages such as Java. It does not have a `try...catch`. The normal practice is to define a global error handling type at the application level:

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

The `thiserror` library is used here, which can easily define your own custom error type and simplify the code. For simple writing, define an `AppResult` by the way.

## Error handling in Handler

In Salvo, `Handler` often encounters various errors, such as: database connection error, file access error, network connection error, etc. For this type of error, the above error handling methods can be used:

```rust
#[handler]
async fn home()-> AppResult<()> {

}
```
Here `home` directly returns an `AppResult<()>`. However, how to display this error? We need to implement `Writer` for the custom error type `AppResult`, in this implementation we can decide How to display errors:

```rust
#[async_trait]
impl Writer for AppError {
    async fn write(mut self, _req: &mut Request, depot: &mut Depot, res: &mut Response) {
        res.render(Text::Plain("I'm a error, hahaha!"));
    }
}
```

`Error` often contains some sensitive information, under normal circumstances, you don't want to be seen by ordinary users, it is too insecure, and there is no privacy at all. However, if you are a developer or webmaster, you may think It's not the same, you want the error to strip the coat naked and let you see the most real error message.

It can be seen that in the `write` method, we can actually get the references of `Request` and `Depot`, which can easily implement the above operation:

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

## Display of error page

The error page that comes with Salvo is sufficient in most cases, it can display Html, Json or Xml page according to the requested data type. However, in some cases, we still expect to customize the display of the error page .

This can be achieved by custom `Catcher`. For a detailed introduction, see the explanation in the [`Catcher`](../core/catcher/) section.