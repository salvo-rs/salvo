use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer};

use crate::extract::{Extractible, Metadata};
use crate::{Depot, Request, Writer};

/// Extracts a JSON body from the request.
pub struct JsonBody<T>(pub T);
impl<T> JsonBody<T> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for JsonBody<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for JsonBody<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> fmt::Debug for JsonBody<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display> Display for JsonBody<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'ex, T> Extractible<'ex> for JsonBody<T>
where
    T: Deserialize<'ex> + Send,
{
    fn metadata() -> &'static Metadata {
        static METADATA: Metadata = Metadata::new("");
        &METADATA
    }
    async fn extract(
        req: &'ex mut Request,
        _depot: &'ex mut Depot,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        req.parse_json().await
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
        _arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        Self::extract(req, depot).await
    }
}

impl<'de, T> Deserialize<'de> for JsonBody<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(JsonBody)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::test::TestClient;

    #[test]
    fn test_json_body_into_inner() {
        let form = JsonBody::<String>("json_body".to_owned());
        assert_eq!(form.into_inner(), "json_body".to_owned());
    }

    #[test]
    fn test_json_body_deref() {
        let form = JsonBody::<String>("json_body".to_owned());
        assert_eq!(form.deref(), &"json_body".to_owned());
    }

    #[test]
    fn test_json_body_deref_mut() {
        let mut form = JsonBody::<String>("json_body".to_owned());
        assert_eq!(form.deref_mut(), &mut "json_body".to_owned());
    }

    #[test]
    fn test_json_body_debug() {
        let form = JsonBody::<String>("json_body".to_owned());
        assert_eq!(format!("{form:?}"), r#""json_body""#);
    }

    #[test]
    fn test_json_body_display() {
        let form = JsonBody::<String>("json_body".to_owned());
        assert_eq!(format!("{form}"), "json_body");
    }

    #[test]
    fn test_json_body_metadata() {
        let metadata = JsonBody::<String>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    async fn test_json_body_extract_with_arg() {
        let map = BTreeMap::from_iter([("key", "value")]);
        let mut req = TestClient::post("http://127.0.0.1:8698/")
            .json(&map)
            .build();
        let mut depot = Depot::new();
        let result =
            JsonBody::<BTreeMap<&str, &str>>::extract_with_arg(&mut req, &mut depot, "key").await;
        assert_eq!("value", result.unwrap().0["key"]);
    }
}
