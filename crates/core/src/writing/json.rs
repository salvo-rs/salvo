use std::fmt::{self, Debug, Display, Formatter};

use async_trait::async_trait;
use serde::Serialize;

use super::{Scribe, try_set_header};
use crate::http::header::{CONTENT_TYPE, HeaderValue};
use crate::http::{Response, StatusError};

/// Write serializable content to response as json content.
///
/// It will set `content-type` to `application/json; charset=utf-8`.
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct User {
///    name: String,
/// }
/// #[handler]
/// async fn hello(res: &mut Response) -> Json<User> {
///     Json(User { name: "jobs".into() })
/// }
/// ```
pub struct Json<T>(pub T);

#[async_trait]
impl<T> Scribe for Json<T>
where
    T: Serialize + Send,
{
    fn render(self, res: &mut Response) {
        match serde_json::to_vec(&self.0) {
            Ok(bytes) => {
                try_set_header(
                    &mut res.headers,
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/json; charset=utf-8"),
                );
                let _ = res.write_body(bytes);
            }
            Err(e) => {
                tracing::error!(error = ?e, "JsonContent write error");
                res.render(StatusError::internal_server_error());
            }
        }
    }
}
impl<T: Debug> Debug for Json<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Json").field(&self.0).finish()
    }
}
impl<T: Display> Display for Json<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;

    use super::*;
    use crate::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_write_json_content() {
        #[derive(Serialize, Debug)]
        struct User {
            name: String,
        }
        #[handler]
        async fn test() -> Json<User> {
            Json(User {
                name: "jobs".into(),
            })
        }

        let router = Router::new().push(Router::with_path("test").get(test));
        let mut res = TestClient::get("http://127.0.0.1:5800/test")
            .send(router)
            .await;
        assert_eq!(res.take_string().await.unwrap(), r#"{"name":"jobs"}"#);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
    }
}
