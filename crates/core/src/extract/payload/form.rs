use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer};

use crate::extract::{Extractible, Metadata};
use crate::{Depot, Request, Writer};

/// Extracts a form body from the request.
pub struct FormBody<T>(pub T);
impl<T> FormBody<T> {
    /// Consumes self and returns the value of the parameter.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for FormBody<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for FormBody<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> fmt::Debug for FormBody<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display> Display for FormBody<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'ex, T> Extractible<'ex> for FormBody<T>
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
        req.parse_form().await
    }
    async fn extract_with_arg(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
        _arg: &str,
    ) -> Result<Self, impl Writer + Send + fmt::Debug + 'static> {
        Self::extract(req, depot).await
    }
}

impl<'de, T> Deserialize<'de> for FormBody<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(FormBody)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::test::TestClient;

    #[test]
    fn test_form_body_into_inner() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(form.into_inner(), "form_body".to_owned());
    }

    #[test]
    fn test_form_body_deref() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(form.deref(), &"form_body".to_owned());
    }

    #[test]
    fn test_form_body_deref_mut() {
        let mut form = FormBody::<String>("form_body".to_owned());
        assert_eq!(form.deref_mut(), &mut "form_body".to_owned());
    }

    #[test]
    fn test_form_body_debug() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(format!("{form:?}"), r#""form_body""#);
    }

    #[test]
    fn test_form_body_display() {
        let form = FormBody::<String>("form_body".to_owned());
        assert_eq!(format!("{form}"), "form_body");
    }

    #[test]
    fn test_form_body_metadata() {
        let metadata = FormBody::<String>::metadata();
        assert_eq!("", metadata.name);
    }

    #[tokio::test]
    async fn test_form_body_extract_with_arg() {
        let map = BTreeMap::from_iter([("key", "value")]);
        let mut req = TestClient::post("http://127.0.0.1:8698/")
            .form(&map)
            .build();
        let mut depot = Depot::new();
        let result =
            FormBody::<BTreeMap<&str, &str>>::extract_with_arg(&mut req, &mut depot, "key").await;
        assert_eq!("value", result.unwrap().0["key"]);
    }
}
