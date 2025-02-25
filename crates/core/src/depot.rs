use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};

/// Store temp data for current request.
///
/// A `Depot` created when server process a request from client. It will dropped when all process
/// for this request done.
///
/// # Example
/// We set `current_user` value in function `set_user` , and then use this value in the following
/// middlewares and handlers.
///
/// ```no_run
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn set_user(depot: &mut Depot) {
///     depot.insert("user", "client");
/// }
/// #[handler]
/// async fn hello(depot: &mut Depot) -> String {
///     format!("Hello {}", depot.get::<&str>("user").copied().unwrap_or_default())
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let router = Router::new().hoop(set_user).goal(hello);
///     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
///     Server::new(acceptor).serve(router).await;
/// }
/// ```

#[derive(Default)]
pub struct Depot {
    map: HashMap<String, Box<dyn Any + Send + Sync>>,
}

#[inline]
fn type_key<T: 'static>() -> String {
    format!("{:?}", TypeId::of::<T>())
}

impl Depot {
    /// Creates an empty `Depot`.
    ///
    /// The depot is initially created with a capacity of 0, so it will not allocate until it is first inserted into.
    #[inline]
    pub fn new() -> Depot {
        Depot {
            map: HashMap::new(),
        }
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
        self.map.insert(type_key::<V>(), Box::new(value));
        self
    }

    /// Obtain a reference to a value previous inject to the depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting failed.
    #[inline]
    pub fn obtain<T: Any + Send + Sync>(&self) -> Result<&T, Option<&Box<dyn Any + Send + Sync>>> {
        self.get(&type_key::<T>())
    }

    /// Obtain a mutable reference to a value previous inject to the depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting failed.
    #[inline]
    pub fn obtain_mut<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<&mut T, Option<&mut Box<dyn Any + Send + Sync>>> {
        self.get_mut(&type_key::<T>())
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
    /// Check is there a value is injected to the depot.
    ///
    /// **Note**: This is only check injected value.
    #[inline]
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.map.contains_key(&type_key::<T>())
    }

    /// Immutably borrows value from depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting failed.
    #[inline]
    pub fn get<V: Any + Send + Sync>(
        &self,
        key: &str,
    ) -> Result<&V, Option<&Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.map.get(key) {
            value.downcast_ref::<V>().ok_or(Some(value))
        } else {
            Err(None)
        }
    }

    /// Mutably borrows value from depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting failed.
    pub fn get_mut<V: Any + Send + Sync>(
        &mut self,
        key: &str,
    ) -> Result<&mut V, Option<&mut Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.map.get_mut(key) {
            if value.downcast_mut::<V>().is_some() {
                Ok(value
                    .downcast_mut::<V>()
                    .expect("downcast_mut shuold not be failed"))
            } else {
                Err(Some(value))
            }
        } else {
            Err(None)
        }
    }

    /// Remove value from depot and returning the value at the key if the key was previously in the depot.
    #[inline]
    pub fn remove<V: Any + Send + Sync>(
        &mut self,
        key: &str,
    ) -> Result<V, Option<Box<dyn Any + Send + Sync>>> {
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

    /// Remove value from depot and returning the value if the type was previously in the depot.
    #[inline]
    pub fn scrape<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<T, Option<Box<dyn Any + Send + Sync>>> {
        self.remove(&type_key::<T>())
    }
}

impl Debug for Depot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Depot")
            .field("keys", &self.map.keys())
            .finish()
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
        assert_eq!(
            depot.get_mut::<String>("one").unwrap(),
            &mut "ONE".to_owned()
        );
    }

    #[tokio::test]
    async fn test_middleware_use_depot() {
        #[handler]
        async fn set_user(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            depot.insert("user", "client");
            ctrl.call_next(req, depot, res).await;
        }
        #[handler]
        async fn hello(depot: &mut Depot) -> String {
            format!(
                "Hello {}",
                depot.get::<&str>("user").copied().unwrap_or_default()
            )
        }
        let router = Router::new().hoop(set_user).goal(hello);
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
