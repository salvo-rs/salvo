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
    map: HashMap<String, Box<dyn Any + Send + Sync>>,
    typed: HashMap<TypeId, TypedEntry>,
}

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
            map: HashMap::new(),
            typed: HashMap::new(),
        }
    }

    /// Returns the named values stored in this `Depot`.
    ///
    /// This only exposes values inserted with explicit string keys. Values inserted through typed
    /// storage are intentionally kept separate.
    #[inline]
    #[must_use]
    pub fn named_entries(&self) -> &HashMap<String, Box<dyn Any + Send + Sync>> {
        &self.map
    }

    /// Returns the named values stored in this `Depot`.
    ///
    /// This is a legacy alias for [`Depot::named_entries`].
    #[inline]
    #[must_use]
    pub fn inner(&self) -> &HashMap<String, Box<dyn Any + Send + Sync>> {
        self.named_entries()
    }

    /// Creates an empty `Depot` with the specified capacity.
    ///
    /// The named storage will be able to hold at least capacity elements without reallocating. If
    /// capacity is 0, the depot will not allocate.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            typed: HashMap::new(),
        }
    }
    /// Returns the total number of named and typed values the depot can hold without reallocating.
    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.map.capacity() + self.typed.capacity()
    }

    /// Returns a read-only view of values stored with explicit string keys.
    #[inline]
    #[must_use]
    pub fn named(&self) -> NamedDepot<'_> {
        NamedDepot { map: &self.map }
    }

    /// Returns a mutable view of values stored with explicit string keys.
    #[inline]
    #[must_use]
    pub fn named_mut(&mut self) -> NamedDepotMut<'_> {
        NamedDepotMut { map: &mut self.map }
    }

    /// Returns a read-only view of values stored by type.
    #[inline]
    #[must_use]
    pub fn typed(&self) -> TypedDepot<'_> {
        TypedDepot { map: &self.typed }
    }

    /// Returns a mutable view of values stored by type.
    #[inline]
    #[must_use]
    pub fn typed_mut(&mut self) -> TypedDepotMut<'_> {
        TypedDepotMut {
            map: &mut self.typed,
        }
    }

    /// Inserts a typed value into the depot.
    ///
    /// The value is stored by its [`TypeId`], separately from string-keyed values. Any existing
    /// value of the same type is replaced.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::Depot;
    ///
    /// #[derive(Clone, Debug, PartialEq)]
    /// struct Config {
    ///     debug: bool,
    /// }
    ///
    /// let mut depot = Depot::new();
    /// depot.insert_typed(Config { debug: true });
    /// assert_eq!(depot.get_typed::<Config>(), Some(&Config { debug: true }));
    /// assert_eq!(depot.remove_typed::<Config>(), Some(Config { debug: true }));
    /// assert!(!depot.contains_typed::<Config>());
    /// ```
    #[inline]
    pub fn insert_typed<V: Any + Send + Sync>(&mut self, value: V) -> &mut Self {
        self.typed.insert(TypeId::of::<V>(), TypedEntry::new(value));
        self
    }

    /// Returns a reference to a typed value stored in the depot.
    #[inline]
    #[must_use]
    pub fn get_typed<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.typed
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_ref::<T>())
    }

    /// Returns a mutable reference to a typed value stored in the depot.
    #[inline]
    #[must_use]
    pub fn get_typed_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.typed
            .get_mut(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_mut::<T>())
    }

    /// Returns true if a typed value is stored in the depot.
    #[inline]
    #[must_use]
    pub fn contains_typed<T: Any + Send + Sync>(&self) -> bool {
        self.typed.contains_key(&TypeId::of::<T>())
    }

    /// Removes and returns a typed value from the depot.
    #[inline]
    pub fn remove_typed<T: Any + Send + Sync>(&mut self) -> Option<T> {
        self.typed.remove(&TypeId::of::<T>()).map(|entry| {
            *entry
                .value
                .downcast::<T>()
                .expect("typed depot value should downcast")
        })
    }

    /// Inject a value into the depot.
    ///
    /// This is an alias for [`Depot::insert_typed`].
    #[inline]
    pub fn inject<V: Any + Send + Sync>(&mut self, value: V) -> &mut Self {
        self.insert_typed(value)
    }

    /// Obtain a reference to a value previously injected into the depot.
    ///
    /// Returns `Err(None)` if the value is not present in the depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if the value is present but downcasting
    /// failed.
    ///
    /// Consider [`Depot::get_typed`], which returns an `Option` instead.
    #[inline]
    pub fn obtain<T: Any + Send + Sync>(&self) -> Result<&T, Option<&Box<dyn Any + Send + Sync>>> {
        if let Some(entry) = self.typed.get(&TypeId::of::<T>()) {
            entry.value.downcast_ref::<T>().ok_or(Some(&entry.value))
        } else {
            Err(None)
        }
    }

    /// Obtain a mutable reference to a value previously injected into the depot.
    ///
    /// Returns `Err(None)` if value is not present in depot.
    /// Returns `Err(Some(Box<dyn Any + Send + Sync>))` if value is present in depot but downcasting
    /// failed.
    ///
    /// Consider [`Depot::get_typed_mut`], which returns an `Option` instead.
    #[inline]
    pub fn obtain_mut<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<&mut T, Option<&mut Box<dyn Any + Send + Sync>>> {
        if let Some(entry) = self.typed.get_mut(&TypeId::of::<T>()) {
            if entry.value.is::<T>() {
                Ok(entry
                    .value
                    .downcast_mut::<T>()
                    .expect("typed depot value should downcast"))
            } else {
                Err(Some(&mut entry.value))
            }
        } else {
            Err(None)
        }
    }

    /// Inserts a key-value pair into the depot.
    ///
    /// Any existing value stored under the same key is replaced.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: Into<String>,
        V: Any + Send + Sync,
    {
        self.map.insert(key.into(), Box::new(value));
        self
    }

    /// Check whether a value is stored in the depot under the given key.
    #[inline]
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
    /// Check whether a value of this type has been injected into the depot.
    ///
    /// **Note**: Only checks typed values, i.e. those inserted via [`Depot::inject`],
    /// [`Depot::insert_typed`] or [`TypedDepotMut::insert`]. This is an alias for
    /// [`Depot::contains_typed`].
    #[inline]
    #[must_use]
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
        if let Some(value) = self.map.get(key) {
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
        if let Some(value) = self.map.get_mut(key) {
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
        self.remove_any(key).is_some()
    }

    /// Removes and returns any value stored under the given key without downcasting it.
    #[inline]
    pub fn remove_any(&mut self, key: &str) -> Option<Box<dyn Any + Send + Sync>> {
        self.map.remove(key)
    }

    /// Remove the injected value of the given type from the depot and return it, if present.
    ///
    /// Consider [`Depot::remove_typed`], which returns an `Option` instead.
    #[inline]
    pub fn scrape<T: Any + Send + Sync>(
        &mut self,
    ) -> Result<T, Option<Box<dyn Any + Send + Sync>>> {
        if let Some(entry) = self.typed.remove(&TypeId::of::<T>()) {
            entry.value.downcast::<T>().map(|b| *b).map_err(Some)
        } else {
            Err(None)
        }
    }
}

/// Read-only access to values stored with explicit string keys.
pub struct NamedDepot<'a> {
    map: &'a HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl<'a> NamedDepot<'a> {
    /// Returns the raw named entries.
    #[inline]
    #[must_use]
    pub fn entries(&self) -> &'a HashMap<String, Box<dyn Any + Send + Sync>> {
        self.map
    }

    /// Returns true if a value is stored under the given key.
    #[inline]
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Returns a reference to a value stored under the given key.
    #[inline]
    #[must_use]
    pub fn get<V: Any + Send + Sync>(&self, key: &str) -> Option<&'a V> {
        self.map
            .get(key)
            .and_then(|value| value.downcast_ref::<V>())
    }
}

impl Debug for NamedDepot<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("NamedDepot")
            .field("keys", &self.map.keys())
            .finish()
    }
}

/// Mutable access to values stored with explicit string keys.
pub struct NamedDepotMut<'a> {
    map: &'a mut HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl NamedDepotMut<'_> {
    /// Returns the raw named entries.
    #[inline]
    #[must_use]
    pub fn entries(&self) -> &HashMap<String, Box<dyn Any + Send + Sync>> {
        self.map
    }

    /// Returns true if a value is stored under the given key.
    #[inline]
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Inserts a key-value pair and returns the old boxed value, if present.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, value: V) -> Option<Box<dyn Any + Send + Sync>>
    where
        K: Into<String>,
        V: Any + Send + Sync,
    {
        self.map.insert(key.into(), Box::new(value))
    }

    /// Returns a reference to a value stored under the given key.
    #[inline]
    #[must_use]
    pub fn get<V: Any + Send + Sync>(&self, key: &str) -> Option<&V> {
        self.map
            .get(key)
            .and_then(|value| value.downcast_ref::<V>())
    }

    /// Returns a mutable reference to a value stored under the given key.
    #[inline]
    #[must_use]
    pub fn get_mut<V: Any + Send + Sync>(&mut self, key: &str) -> Option<&mut V> {
        self.map
            .get_mut(key)
            .and_then(|value| value.downcast_mut::<V>())
    }

    /// Removes and returns a value stored under the given key.
    ///
    /// If the stored value has a different type, it is left in the depot and `None` is returned.
    #[inline]
    pub fn remove<V: Any + Send + Sync>(&mut self, key: &str) -> Option<V> {
        if self.map.get(key).is_some_and(|value| value.is::<V>()) {
            self.map
                .remove(key)
                .and_then(|value| value.downcast::<V>().ok())
                .map(|value| *value)
        } else {
            None
        }
    }

    /// Removes and returns any value stored under the given key without downcasting it.
    #[inline]
    pub fn remove_any(&mut self, key: &str) -> Option<Box<dyn Any + Send + Sync>> {
        self.map.remove(key)
    }
}

impl Debug for NamedDepotMut<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("NamedDepotMut")
            .field("keys", &self.map.keys())
            .finish()
    }
}

/// Read-only access to values stored by type.
pub struct TypedDepot<'a> {
    map: &'a HashMap<TypeId, TypedEntry>,
}

impl<'a> TypedDepot<'a> {
    /// Returns true if a value of this type is stored in the depot.
    #[inline]
    #[must_use]
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Returns a reference to a value stored by type.
    #[inline]
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&'a T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_ref::<T>())
    }

    /// Returns the Rust type names of values stored by type.
    #[inline]
    pub fn type_names(&self) -> impl Iterator<Item = &'static str> + 'a {
        self.map.values().map(|entry| entry.type_name)
    }
}

impl Debug for TypedDepot<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let typed = self
            .map
            .values()
            .map(|entry| entry.type_name)
            .collect::<Vec<_>>();
        f.debug_struct("TypedDepot").field("types", &typed).finish()
    }
}

/// Mutable access to values stored by type.
pub struct TypedDepotMut<'a> {
    map: &'a mut HashMap<TypeId, TypedEntry>,
}

impl TypedDepotMut<'_> {
    /// Returns true if a value of this type is stored in the depot.
    #[inline]
    #[must_use]
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Inserts a typed value and returns the old value of the same type, if present.
    #[inline]
    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), TypedEntry::new(value))
            .map(|entry| {
                *entry
                    .value
                    .downcast::<T>()
                    .expect("typed depot value should downcast")
            })
    }

    /// Returns a reference to a value stored by type.
    #[inline]
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_ref::<T>())
    }

    /// Returns a mutable reference to a value stored by type.
    #[inline]
    #[must_use]
    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_mut::<T>())
    }

    /// Removes and returns a value stored by type.
    #[inline]
    pub fn remove<T: Any + Send + Sync>(&mut self) -> Option<T> {
        self.map.remove(&TypeId::of::<T>()).map(|entry| {
            *entry
                .value
                .downcast::<T>()
                .expect("typed depot value should downcast")
        })
    }

    /// Returns the Rust type names of values stored by type.
    #[inline]
    pub fn type_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.map.values().map(|entry| entry.type_name)
    }
}

impl Debug for TypedDepotMut<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let typed = self
            .map
            .values()
            .map(|entry| entry.type_name)
            .collect::<Vec<_>>();
        f.debug_struct("TypedDepotMut")
            .field("types", &typed)
            .finish()
    }
}

impl Debug for Depot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let typed = self
            .typed
            .values()
            .map(|entry| entry.type_name)
            .collect::<Vec<_>>();
        f.debug_struct("Depot")
            .field("named_keys", &self.map.keys())
            .field("typed", &typed)
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
        assert!(depot.named().contains_key("one"));

        assert_eq!(depot.get::<String>("one").unwrap(), &"ONE".to_owned());
        assert_eq!(depot.named().get::<String>("one").unwrap(), "ONE");
        assert_eq!(
            depot.get_mut::<String>("one").unwrap(),
            &mut "ONE".to_owned()
        );
        assert_eq!(
            depot.named_mut().get_mut::<String>("one").unwrap(),
            &mut "ONE".to_owned()
        );
    }

    #[test]
    fn test_typed_depot() {
        let mut depot = Depot::new();

        assert!(depot.get_typed::<String>().is_none());
        depot.insert_typed("typed".to_owned());
        assert!(depot.contains_typed::<String>());
        assert!(depot.contains::<String>());
        assert_eq!(depot.get_typed::<String>().unwrap(), "typed");
        assert_eq!(depot.typed().get::<String>().unwrap(), "typed");
        assert_eq!(depot.obtain::<String>().unwrap(), "typed");

        let old = depot.typed_mut().insert("new typed".to_owned());
        assert_eq!(old.as_deref(), Some("typed"));
        assert_eq!(depot.get_typed::<String>().unwrap(), "new typed");

        assert_eq!(depot.remove_typed::<String>().unwrap(), "new typed");
        assert!(depot.obtain::<String>().is_err());
    }

    #[test]
    fn test_named_and_typed_storage_are_separate() {
        let mut depot = Depot::new();

        depot.insert("value", "named".to_owned());
        depot.insert_typed("typed".to_owned());

        assert_eq!(depot.get::<String>("value").unwrap(), "named");
        assert_eq!(depot.get_typed::<String>().unwrap(), "typed");
        assert_eq!(depot.named_entries().len(), 1);

        let removed = depot.remove_any("value").unwrap();
        assert_eq!(*removed.downcast::<String>().unwrap(), "named");
        assert_eq!(depot.remove_typed::<String>().unwrap(), "typed");
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
