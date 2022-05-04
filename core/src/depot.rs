use std::any::Any;
use std::collections::HashMap;
use std::fmt::{self, Formatter};

/// Depot if for store temp data of current request. Each handler can read or write data to it.
/// 
/// # Example
/// 
/// ```no_run
/// use salvo_core::prelude::*;
/// 
/// #[fn_handler]
/// async fn set_user(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
///     depot.insert("user", "client");
///     ctrl.call_next(req, depot, res).await;
/// }
/// #[fn_handler]
/// async fn hello_world(depot: &mut Depot) -> String {
///     format!("Hello {}", depot.get::<&str>("user").map(|s|*s).unwrap_or_default())
/// }
/// #[tokio::main]
/// async fn main() {
///     let router = Router::new().hoop(set_user).handle(hello_world);
///     Server::new(TcpListener::bind("0.0.0.0:7878")).serve(router).await;
/// }
/// ```
#[derive(Default)]
pub struct Depot {
    data: HashMap<String, Box<dyn Any + Send>>,
}

impl Depot {
    /// Creates an empty ```Depot```.
    ///
    /// The depot is initially created with a capacity of 0, so it will not allocate until it is first inserted into.
    #[inline]
    pub fn new() -> Depot {
        Depot { data: HashMap::new() }
    }

    /// Creates an empty ```Depot``` with the specified capacity.
    ///
    /// The depot will be able to hold at least capacity elements without reallocating. If capacity is 0, the depot will not allocate.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Depot {
        Depot {
            data: HashMap::with_capacity(capacity),
        }
    }
    /// Returns the number of elements the depot can hold without reallocating.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }
    /// Inserts a key-value pair into the depot.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Any + Send + Sync,
    {
        self.data.insert(key.into(), Box::new(value));
    }

    /// Check is there a value stored in depot with this key.
    #[inline]
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Immutably borrows value from depot, returing none if value is not present in depot.
    #[inline]
    pub fn get<V>(&self, key: &str) -> Option<&V>
    where
        V: Any + Send,
    {
        self.data.get(key).and_then(|b| b.downcast_ref::<V>())
    }

    /// Mutably borrows value from depot, returing none if value is not present in depot.
    #[inline]
    pub fn get_mut<V>(&mut self, key: &str) -> Option<&mut V>
    where
        V: Any + Send,
    {
        self.data.get_mut(key).and_then(|b| b.downcast_mut::<V>())
    }

    /// Take value from depot container.
    #[inline]
    pub fn remove<V>(&mut self, key: &str) -> Option<V>
    where
        V: Any + Send,
    {
        self.data.remove(key).and_then(|b| b.downcast::<V>().ok()).map(|b| *b)
    }

    /// Transfer all data to a new instance.
    #[inline]
    pub fn transfer(&mut self) -> Depot {
        let mut data = HashMap::with_capacity(self.data.len());
        for (k, v) in self.data.drain() {
            data.insert(k, v);
        }
        Depot { data }
    }
}

impl fmt::Debug for Depot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Depot").field("keys", &self.data.keys()).finish()
    }
}

#[cfg(test)]
mod test {
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
}
