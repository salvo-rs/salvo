use std::any::Any;
use std::collections::HashMap;
use std::fmt;

/// Depot if for store temp data of current request. Each handler can read or write data to it.
///
pub struct Depot {
    data: HashMap<String, Box<dyn Any + Send>>,
}

impl Depot {
    /// Creates an empty ```Depot```.
    ///
    /// The depot is initially created with a capacity of 0, so it will not allocate until it is first inserted into.
    pub fn new() -> Depot {
        Depot { data: HashMap::new() }
    }
    /// Creates an empty ```Depot``` with the specified capacity.
    ///
    /// The depot will be able to hold at least capacity elements without reallocating. If capacity is 0, the depot will not allocate.
    pub fn with_capacity(capacity: usize) -> Depot {
        Depot {
            data: HashMap::with_capacity(capacity),
        }
    }

    /// Inserts a key-value pair into the depot.
    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.insert(key.into(), Box::new(value));
    }

    /// Check is there a value stored in depot with this key.
    pub fn has<K>(&self, key: K) -> bool
    where
        K: Into<String>,
    {
        self.data.get(&key.into()).is_some()
    }

    /// Immutably borrows value from depot, returing none if value is not present in depot.
    pub fn try_borrow<K, V>(&self, key: K) -> Option<&V>
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.get(&key.into()).and_then(|b| b.downcast_ref::<V>())
    }

    /// Immutably borrows value from depot.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently mutably borrowed or not present in depot. For a non-panicking variant, use
    /// [`try_borrow`](#method.try_borrow).
    pub fn borrow<K, V>(&self, key: K) -> &V
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.try_borrow(key)
            .expect("required type is not present in depot container or mutably borrowed.")
    }

    /// Mutably borrows value from depot, returing none if value is not present in depot.
    pub fn try_borrow_mut<K, V>(&mut self, key: K) -> Option<&mut V>
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.get_mut(&key.into()).and_then(|b| b.downcast_mut::<V>())
    }

    ///Mutably borrows value from depot.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently borrowed or not present in depot. For a non-panicking variant, use
    /// [`try_borrow_mut`](#method.try_borrow_mut).
    pub fn borrow_mut<K, V>(&mut self, key: K) -> &mut V
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.try_borrow_mut(key)
            .expect("required type is not present in depot container or currently borrowed.")
    }

    /// Take value from depot container.
    pub fn try_take<K, V>(&mut self, key: K) -> Option<V>
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data
            .remove(&key.into())
            .and_then(|b| b.downcast::<V>().ok())
            .map(|b| *b)
    }

    /// Take value from depot container.
    ///
    /// # Panics
    ///
    /// Panics if the value is not present in depot container. For a non-panicking variant, use
    /// [`try_take`](#method.try_take).
    pub fn take<K, V>(&mut self, key: K) -> V
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.try_take(key)
            .expect("required type is not present in depot container")
    }

    pub fn transfer(&mut self) -> Depot {
        let mut data = HashMap::with_capacity(self.data.len());
        for (k, v) in self.data.drain() {
            data.insert(k, v);
        }
        Depot { data }
    }
}

impl fmt::Debug for Depot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Depot").finish()
    }
}
