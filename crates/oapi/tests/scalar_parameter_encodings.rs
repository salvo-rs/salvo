//! Supported scalar styles retain complete values and accept their advertised sources.
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_oapi::{Components, ToParameters};
use serde_json::json;

#[derive(Debug, serde::Deserialize, ToParameters)]
struct Scalars {
    #[salvo(parameter(style = Form))]
    query: String,
    #[salvo(parameter(parameter_in = Header, style = Simple))]
    header: String,
    #[salvo(parameter(parameter_in = Path, style = Simple))]
    path: u32,
    #[salvo(parameter(parameter_in = Cookie, style = Cookie))]
    cookie: String,
}

#[tokio::test]
async fn supported_scalar_styles_preserve_commas() {
    let parameters = json!(Scalars::to_parameters(&mut Components::new()));
    assert_eq!(parameters[0]["style"], "form");
    assert_eq!(parameters[1]["style"], "simple");
    assert_eq!(parameters[2]["style"], "simple");
    assert_eq!(parameters[3]["style"], "cookie");
    let mut req = TestClient::get("http://localhost/42?query=one%2Ctwo")
        .add_header("header", "three,four", true)
        .add_header("cookie", "cookie=five,six", true)
        .build();
    req.params_mut().insert("path", "42".into());
    let parsed = Scalars::extract(&mut req, &mut Depot::new()).await.unwrap();
    assert_eq!(parsed.query, "one,two");
    assert_eq!(parsed.header, "three,four");
    assert_eq!(parsed.path, 42);
    assert_eq!(parsed.cookie, "five,six");
}
