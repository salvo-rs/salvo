use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::{self, Formatter};

/// Depot is for store temp data of current request. Each handler can read or write data to it.
///
/// # Example
///
/// ```no_run
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn set_user(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
///     depot.insert("user", "client");
///     ctrl.call_next(req, depot, res).await;
/// }
/// #[handler]
/// async fn hello(depot: &mut Depot) -> String {
///     format!("Hello {}", depot.get::<&str>("user").map(|s|*s).unwrap_or_default())
/// }
/// #[tokio::main]
/// async fn main() {
///     let router = Router::new().hoop(set_user).handle(hello);
///     let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
///     Server::new(acceptor).serve(router).await;
/// }
/// ```

#[derive(Default)]
pub struct Depot {
    map: HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl Depot {
    /// Creates an empty `Depot`.
    ///
    /// The depot is initially created with a capacity of 0, so it will not allocate until it is first inserted into.
    #[inline]
    pub fn new() -> Depot {
        Depot { map: HashMap::new() }
    }

    /// Get reference to depot inner map.
    #[inline]
    pub fn inner(&self) -> &HashMap<String, Box<dyn Any + Send + Sync>> {
        &self.map
    }

    /// Creates an empty `Depot` with the specified capacity.
    ///
    /// The depot will be able to hold at least capacity elements without reallocating. If capacity is 0, the depot will not allocate.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Depot {
            map: HashMap::with_capacity(capacity),
        }
    }
    /// Returns the number of elements the depot can hold without reallocating.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }

    /// Inject a value into the depot.
    #[inline]
    pub fn inject<V: Any + Send + Sync>(&mut self, value: V) -> &mut Self {
        self.map.insert(format!("{:?}", TypeId::of::<V>()), Box::new(value));
        self
    }

    /// Obtain a reference to a value previous inject to the depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcast failed.
    #[inline]
    pub fn obtain<T: Any + Send + Sync>(&self) -> Result<&T, Option<&Box<dyn Any + Send + Sync>>> {
        self.get(&format!("{:?}", TypeId::of::<T>()))
    }

    /// Obtain a mutable reference to a value previous inject to the depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcast failed.
    #[inline]
    pub fn obtain_mut<T: Any + Send + Sync>(&mut self) -> Result<&mut T, Option<&mut Box<dyn Any + Send + Sync>>> {
        self.get_mut(&format!("{:?}", TypeId::of::<T>()))
    }

    /// Inserts a key-value pair into the depot.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: Any + Send + Sync,
    {
        self.map.insert(key.into(), Box::new(value));
        self
    }

    /// Check is there a value stored in depot with this key.
    #[inline]
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Immutably borrows value from depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcast failed.
    #[inline]
    pub fn get<V: Any + Send + Sync>(&self, key: &str) -> Result<&V, Option<&Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.map.get(key) {
            value.downcast_ref::<V>().ok_or(Some(value))
        } else {
            Err(None)
        }
    }

    /// Mutably borrows value from depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcast failed.
    #[inline]
    pub fn get_mut<V: Any + Send + Sync>(
        &mut self,
        key: &str,
    ) -> Result<&mut V, Option<&mut Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.map.get_mut(key) {
            if value.downcast_mut::<V>().is_some() {
                return Ok(value.downcast_mut::<V>().unwrap());
            } else {
                Err(Some(value))
            }
        } else {
            Err(None)
        }
    }

    /// Remove value from depot and returning the value at the key if the key was previously in the depot.
    #[inline]
    pub fn remove<V: Any + Send + Sync>(&mut self, key: &str) -> Result<V, Option<Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.map.remove(key) {
            value.downcast::<V>().map(|b| *b).map_err(Some)
        } else {
            Err(None)
        }
    }

    /// Delete the key from depot, if the key is not present, return `false`.
    #[inline]
    pub fn delete(&mut self, key: &str) -> bool {
        self.map.remove(key).is_some()
    }

    /// Transfer all data to a new instance.
    #[inline]
    pub fn transfer(&mut self) -> Self {
        let mut map = HashMap::with_capacity(self.map.len());
        for (k, v) in self.map.drain() {
            map.insert(k, v);
        }
        Self { map }
    }
}

impl fmt::Debug for Depot {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Depot").field("keys", &self.map.keys()).finish()
    }
}

#[cfg(test)]
mod test {
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    use super::*;

    #[test]
    fn test_depot() {
        let mut depot = Depot::with_capacity(6);
        assert!(depot.capacity() >= 6);

        depot.insert("one", "ONE".to_owned());
        assert!(depot.contains_key("one"));

        assert_eq!(depot.get::<String>("one").unwrap(), &"ONE".to_owned());
        assert_eq!(depot.get_mut::<String>("one").unwrap(), &mut "ONE".to_owned());
    }

    #[test]
    fn test_transfer() {
        let mut depot = Depot::with_capacity(6);
        depot.insert("one", "ONE".to_owned());

        let depot = depot.transfer();
        assert_eq!(depot.get::<String>("one").unwrap(), &"ONE".to_owned());
    }

    #[tokio::test]
    async fn test_middleware_use_depot() {
        #[handler]
        async fn set_user(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
            depot.insert("user", "client");
            ctrl.call_next(req, depot, res).await;
        }
        #[handler]
        async fn hello(depot: &mut Depot) -> String {
            format!("Hello {}", depot.get::<&str>("user").copied().unwrap_or_default())
        }
        let router = Router::new().hoop(set_user).handle(hello);
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5800")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "Hello client");
    }
}
