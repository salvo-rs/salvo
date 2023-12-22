//! affix middleware is used to add any data to depot.
//!
//! Read more: <https://salvo.rs>

use std::any::TypeId;

use salvo_core::handler;
use salvo_core::prelude::*;

trait Affix {
    fn attach(&self, depot: &mut Depot);
}

#[derive(Default)]
struct AffixCell<V> {
    key: String,
    value: V,
}
impl<T> Affix for AffixCell<T>
where
    T: Send + Sync + Clone + 'static,
{
    fn attach(&self, depot: &mut Depot) {
        depot.insert(self.key.clone(), self.value.clone());
    }
}

/// Inject a value into depot.
#[inline]
pub fn inject<V: Send + Sync + Clone + 'static>(value: V) -> AffixList {
    insert(format!("{:?}", TypeId::of::<V>()), value)
}

/// Insert a key-value pair into depot.
#[inline]
pub fn insert<K, V>(key: K, value: V) -> AffixList
where
    K: Into<String>,
    V: Send + Sync + Clone + 'static,
{
    AffixList::new().insert(key, value)
}

/// AffixList is used to add any data to depot.
#[derive(Default)]
pub struct AffixList(Vec<Box<dyn Affix + Send + Sync + 'static>>);

#[handler]
impl AffixList {
    /// Create an empty affix list.
    pub fn new() -> AffixList {
        AffixList(Vec::new())
    }
    /// Inject a value into depot.
    pub fn inject<V: Send + Sync + Clone + 'static>(self, value: V) -> Self {
        self.insert(format!("{:?}", TypeId::of::<V>()), value)
    }

    /// Insert a key-value pair into depot.
    pub fn insert<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Send + Sync + Clone + 'static,
    {
        let cell = AffixCell { key: key.into(), value };
        self.0.push(Box::new(cell));
        self
    }
    async fn handle(&self, depot: &mut Depot) {
        for cell in &self.0 {
            cell.attach(depot);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    struct User {
        name: String,
    }
    #[handler]
    async fn hello(depot: &mut Depot) -> String {
        format!(
            "{}:{}",
            depot.obtain::<Arc<User>>().map(|u| u.name.clone()).unwrap_or_default(),
            depot.get::<&str>("data1").copied().unwrap_or_default()
        )
    }
    #[tokio::test]
    async fn test_affix() {
        let user = User {
            name: "salvo".to_string(),
        };
        let router = Router::with_hoop(inject(Arc::new(user)).insert("data1", "powerful")).goal(hello);
        let content = TestClient::get("http://127.0.0.1:5800/")
            .send(router)
            .await
            .take_string()
            .await;
        assert_eq!(content.unwrap(), "salvo:powerful");
    }
}
