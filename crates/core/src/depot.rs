use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};

/// Store temporary data for the current request.
///
/// A `Depot` is created when the server processes a request from a client, and dropped
/// when all processing for the request is finished.
///
/// # Example
/// We set the `current_user` value in function `set_user`, and then use this value in the following
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
///     format!(
///         "Hello {}",
///         depot.get::<&str>("user").copied().unwrap_or_default()
///     )
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let router = Router::new().hoop(set_user).goal(hello);
///     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
///     Server::new(acceptor).serve(router).await;
/// }
/// ```

#[derive(Default)]
pub struct Depot {
    /// Values stored under an explicit string key.
    named: HashMap<String, Box<dyn Any + Send + Sync>>,
    /// Values stored by their type, keyed on [`TypeId`].
    typed: HashMap<TypeId, TypedEntry>,
}

/// A type-keyed value, tagged with its Rust type name for diagnostics.
struct TypedEntry {
    type_name: &'static str,
    value: Box<dyn Any + Send + Sync>,
}

impl TypedEntry {
    #[inline]
    fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self {
            type_name: type_name::<T>(),
            value: Box::new(value),
        }
    }
}

impl Depot {
    /// Creates an empty `Depot`.
    ///
    /// The depot is initially created with a capacity of 0, so it will not allocate until it is
    /// first inserted into.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            named: HashMap::new(),
            typed: HashMap::new(),
        }
    }

    /// Get reference to the depot's inner map of string-keyed values.
    ///
    /// **Note**: this exposes only values inserted with an explicit string key; values stored by
    /// type (via [`Depot::insert_typed`]) are kept in separate storage and are not included.
    #[inline]
    #[must_use]
    pub fn inner(&self) -> &HashMap<String, Box<dyn Any + Send + Sync>> {
        &self.named
    }

    /// Creates an empty `Depot` with the specified capacity for string-keyed values.
    ///
    /// The depot will be able to hold at least capacity string-keyed elements without reallocating.
    /// If capacity is 0, the depot will not allocate.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            named: HashMap::with_capacity(capacity),
            typed: HashMap::new(),
        }
    }
    /// Returns the number of string-keyed elements the depot can hold without reallocating.
    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.named.capacity()
    }

    /// Store a value in the depot, keyed by its type.
    #[inline]
    pub fn insert_typed<V: Any + Send + Sync>(&mut self, value: V) -> &mut Self {
        self.typed.insert(TypeId::of::<V>(), TypedEntry::new(value));
        self
    }

    /// Deprecated alias for [`Depot::insert_typed`].
    #[inline]
    #[deprecated(since = "0.94.0", note = "use `Depot::insert_typed` instead")]
    pub fn inject<V: Any + Send + Sync>(&mut self, value: V) -> &mut Self {
        self.insert_typed(value)
    }

    /// Get a reference to the value of the given type, previously stored by type.
    ///
    /// Returns `Err(None)` if the value is not present in the depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if the value is present but downcasting
    /// failed.
    #[inline]
    pub fn get_typed<T: Any + Send + Sync>(
        &self,
    ) -> Result<&T, Option<&Box<dyn Any + Send + Sync>>> {
        if let Some(entry) = self.typed.get(&TypeId::of::<T>()) {
            entry.value.downcast_ref::<T>().ok_or(Some(&entry.value))
        } else {
            Err(None)
        }
    }

    /// Deprecated alias for [`Depot::get_typed`].
    #[inline]
    #[deprecated(since = "0.94.0", note = "use `Depot::get_typed` instead")]
    pub fn obtain<T: Any + Send + Sync>(&self) -> Result<&T, Option<&Box<dyn Any + Send + Sync>>> {
        self.get_typed::<T>()
    }

    /// Get a mutable reference to the value of the given type, previously stored by type.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting
    /// failed.
    #[inline]
    pub fn get_typed_mut<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<&mut T, Option<&mut Box<dyn Any + Send + Sync>>> {
        if let Some(entry) = self.typed.get_mut(&TypeId::of::<T>()) {
            if entry.value.is::<T>() {
                Ok(entry
                    .value
                    .downcast_mut::<T>()
                    .expect("downcast_mut should not fail"))
            } else {
                Err(Some(&mut entry.value))
            }
        } else {
            Err(None)
        }
    }

    /// Deprecated alias for [`Depot::get_typed_mut`].
    #[inline]
    #[deprecated(since = "0.94.0", note = "use `Depot::get_typed_mut` instead")]
    pub fn obtain_mut<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<&mut T, Option<&mut Box<dyn Any + Send + Sync>>> {
        self.get_typed_mut::<T>()
    }

    /// Inserts a key-value pair into the depot.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: Any + Send + Sync,
    {
        self.named.insert(key.into(), Box::new(value));
        self
    }

    /// Check whether a value is stored in the depot under the given key.
    #[inline]
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.named.contains_key(key)
    }
    /// Check whether a value of the given type has been stored in the depot.
    ///
    /// **Note**: Only checks values inserted via [`Depot::insert_typed`].
    #[inline]
    #[must_use]
    pub fn contains_typed<T: Any + Send + Sync>(&self) -> bool {
        self.typed.contains_key(&TypeId::of::<T>())
    }

    /// Deprecated alias for [`Depot::contains_typed`].
    #[inline]
    #[must_use]
    #[deprecated(since = "0.94.0", note = "use `Depot::contains_typed` instead")]
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.contains_typed::<T>()
    }

    /// Immutably borrows value from depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting
    /// failed.
    #[inline]
    pub fn get<V: Any + Send + Sync>(
        &self,
        key: &str,
    ) -> Result<&V, Option<&Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.named.get(key) {
            value.downcast_ref::<V>().ok_or(Some(value))
        } else {
            Err(None)
        }
    }

    /// Mutably borrows value from depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting
    /// failed.
    pub fn get_mut<V: Any + Send + Sync>(
        &mut self,
        key: &str,
    ) -> Result<&mut V, Option<&mut Box<dyn Any + Send + Sync>>> {
        if let Some(value) = self.named.get_mut(key) {
            if value.downcast_mut::<V>().is_some() {
                Ok(value
                    .downcast_mut::<V>()
                    .expect("downcast_mut should not be failed"))
            } else {
                Err(Some(value))
            }
        } else {
            Err(None)
        }
    }

    /// Remove the value at the given key from the depot and return it, if present.
    ///
    /// The value is returned in its type-erased box; downcast it with
    /// [`Box::downcast`] if you need the concrete type back. Returns `None` if the key was not
    /// present.
    #[inline]
    pub fn remove(&mut self, key: &str) -> Option<Box<dyn Any + Send + Sync>> {
        self.named.remove(key)
    }

    /// Deprecated: use [`Depot::remove`] and check the [`Option`] (e.g. `remove(key).is_some()`).
    #[inline]
    #[deprecated(
        since = "0.94.0",
        note = "use `Depot::remove` and check the returned `Option`"
    )]
    pub fn delete(&mut self, key: &str) -> bool {
        self.remove(key).is_some()
    }

    /// Remove the value of the given type from the depot and return it, if present.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting
    /// failed.
    #[inline]
    pub fn remove_typed<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<T, Option<Box<dyn Any + Send + Sync>>> {
        if let Some(entry) = self.typed.remove(&TypeId::of::<T>()) {
            entry.value.downcast::<T>().map(|b| *b).map_err(Some)
        } else {
            Err(None)
        }
    }

    /// Deprecated alias for [`Depot::remove_typed`].
    #[inline]
    #[deprecated(since = "0.94.0", note = "use `Depot::remove_typed` instead")]
    pub fn scrape<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<T, Option<Box<dyn Any + Send + Sync>>> {
        self.remove_typed::<T>()
    }
}

impl Debug for Depot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let types = self
            .typed
            .values()
            .map(|entry| entry.type_name)
            .collect::<Vec<_>>();
        f.debug_struct("Depot")
            .field("keys", &self.named.keys())
            .field("types", &types)
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

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

    #[test]
    fn test_depot_typed() {
        let mut depot = Depot::new();

        assert!(depot.get_typed::<String>().is_err());
        depot.insert_typed("typed".to_owned());
        assert!(depot.contains_typed::<String>());
        assert_eq!(depot.get_typed::<String>().unwrap(), "typed");
        assert_eq!(depot.get_typed_mut::<String>().unwrap(), "typed");
        assert_eq!(depot.remove_typed::<String>().unwrap(), "typed");
        assert!(!depot.contains_typed::<String>());
    }

    #[test]
    fn test_depot_named_and_typed_are_separate() {
        let mut depot = Depot::new();

        // A string-keyed value and a typed value of the same type don't collide.
        depot.insert("value", "named".to_owned());
        depot.insert_typed("typed".to_owned());

        assert_eq!(depot.get::<String>("value").unwrap(), "named");
        assert_eq!(depot.get_typed::<String>().unwrap(), "typed");
        // `inner()` exposes only string-keyed values.
        assert_eq!(depot.inner().len(), 1);
        assert!(depot.contains_key("value"));

        // `remove` drops the named entry without touching the typed one.
        assert_eq!(
            depot
                .remove("value")
                .and_then(|v| v.downcast::<String>().ok()),
            Some(Box::new("named".to_owned()))
        );
        assert!(depot.remove("value").is_none());
        assert!(!depot.contains_key("value"));
        assert_eq!(depot.get_typed::<String>().unwrap(), "typed");
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

        let content = TestClient::get("http://127.0.0.1:8698")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "Hello client");
    }
}
