//! Middleware for adding shared application state to the request context.
//!
//! This middleware allows you to inject any data into the Depot, making it
//! available to all subsequent handlers in the request pipeline. This is useful
//! for sharing configuration, database connections, and other application state.
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::sync::Mutex;
//!
//! use salvo_core::prelude::*;
//! use salvo_extra::affix_state;
//!
//! #[allow(dead_code)]
//! #[derive(Default, Clone, Debug)]
//! struct Config {
//!     username: String,
//!     password: String,
//! }
//!
//! #[derive(Default, Debug)]
//! struct State {
//!     fails: Mutex<Vec<String>>,
//! }
//!
//! #[handler]
//! async fn hello(depot: &mut Depot) -> String {
//!     let config = depot.obtain::<Config>().unwrap();
//!     let custom_data = depot.get::<&str>("custom_data").unwrap();
//!     let state = depot.obtain::<Arc<State>>().unwrap();
//!     let mut fails_ref = state.fails.lock().unwrap();
//!     fails_ref.push("fail message".into());
//!     format!("Hello World\nConfig: {config:#?}\nFails: {fails_ref:#?}\nCustom Data: {custom_data}")
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = Config {
//!         username: "root".to_string(),
//!         password: "pwd".to_string(),
//!     };
//!     let router = Router::new()
//!         .hoop(
//!             affix_state::inject(config)
//!                 .inject(Arc::new(State {
//!                     fails: Mutex::new(Vec::new()),
//!                 }))
//!                 .insert("custom_data", "I love this world!"),
//!         )
//!         .get(hello)
//!         .push(Router::with_path("hello").get(hello));
//!
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```

use std::any::TypeId;

use salvo_core::handler;
use salvo_core::prelude::*;

trait AffixState {
    fn affix_to(&self, depot: &mut Depot);
}

#[derive(Default)]
struct AffixCell<V> {
    key: String,
    value: V,
}
impl<T> AffixState for AffixCell<T>
where
    T: Send + Sync + Clone + 'static,
{
    fn affix_to(&self, depot: &mut Depot) {
        depot.insert(self.key.clone(), self.value.clone());
    }
}

/// Inject a typed value into depot using the type's ID as the key.
/// 
/// This is useful when you want to access the value by its type rather than by an explicit key.
/// 
/// View [module level documentation](index.html) for more details.
#[inline]
pub fn inject<V: Send + Sync + Clone + 'static>(value: V) -> AffixList {
    insert(format!("{:?}", TypeId::of::<V>()), value)
}

/// Insert a key-value pair into depot with an explicit key.
/// 
/// Use this when you need to access the value using a specific key string.
/// 
/// View [module level documentation](index.html) for more details.
#[inline]
pub fn insert<K, V>(key: K, value: V) -> AffixList
where
    K: Into<String>,
    V: Send + Sync + Clone + 'static,
{
    AffixList::new().insert(key, value)
}

/// AffixList is used to add any data to depot.
/// 
/// View [module level documentation](index.html) for more details.
#[derive(Default)]
pub struct AffixList(Vec<Box<dyn AffixState + Send + Sync + 'static>>);

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
            cell.affix_to(depot);
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
