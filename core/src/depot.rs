use std::any::Any;
use std::collections::HashMap;
use std::fmt;

/// Depot if for store temp data of current request. Each handler can read or write data to it.
///
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
        V: Any + Send,
    {
        self.data.insert(key.into(), Box::new(value));
    }

    /// Check is there a value stored in depot with this key.
    #[inline]
    pub fn has(&self, key: &str) -> bool {
        self.data.get(key).is_some()
    }

    /// Immutably borrows value from depot, returing none if value is not present in depot.
    #[inline]
    pub fn try_borrow<V>(&self, key: &str) -> Option<&V>
    where
        V: Any + Send,
    {
        self.data.get(key).and_then(|b| b.downcast_ref::<V>())
    }

    /// Immutably borrows value from depot.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently mutably borrowed or not present in depot. For a non-panicking variant, use
    /// [`try_borrow`](#method.try_borrow).
    #[inline]
    pub fn borrow<V>(&self, key: &str) -> &V
    where
        V: Any + Send,
    {
        self.try_borrow(key)
            .expect("required type is not present in depot container or mutably borrowed.")
    }

    /// Mutably borrows value from depot, returing none if value is not present in depot.
    #[inline]
    pub fn try_borrow_mut<V>(&mut self, key: &str) -> Option<&mut V>
    where
        V: Any + Send,
    {
        self.data.get_mut(key).and_then(|b| b.downcast_mut::<V>())
    }

    ///Mutably borrows value from depot.
    ///
    /// # Panics
    ///
    /// Panics if the value is currently borrowed or not present in depot. For a non-panicking variant, use
    /// [`try_borrow_mut`](#method.try_borrow_mut).
    #[inline]
    pub fn borrow_mut<V>(&mut self, key: &str) -> &mut V
    where
        V: Any + Send,
    {
        self.try_borrow_mut(key)
            .expect("required type is not present in depot container or currently borrowed.")
    }

    /// Take value from depot container.
    #[inline]
    pub fn try_take<V>(&mut self, key: &str) -> Option<V>
    where
        V: Any + Send,
    {
        self.data.remove(key).and_then(|b| b.downcast::<V>().ok()).map(|b| *b)
    }

    /// Take value from depot container.
    ///
    /// # Panics
    ///
    /// Panics if the value is not present in depot container. For a non-panicking variant, use
    /// [`try_take`](#method.try_take).
    #[inline]
    pub fn take<V>(&mut self, key: &str) -> V
    where
        V: Any + Send,
    {
        self.try_take(key)
            .expect("required type is not present in depot container")
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Depot").finish()
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
        assert!(depot.has("one"));

        assert_eq!(depot.try_borrow::<String>("one"), Some(&"ONE".to_owned()));
        assert_eq!(depot.borrow_mut::<String>("one"), &mut "ONE".to_owned());
        assert_eq!(depot.borrow::<String>("one"), &"ONE".to_owned());
        assert_eq!(depot.take::<String>("one"), "ONE".to_owned());
    }

    #[test]
    fn test_transfer() {
        let mut depot = Depot::with_capacity(6);
        depot.insert("one", "ONE".to_owned());

        let depot = depot.transfer();
        assert_eq!(depot.borrow::<String>("one"), &"ONE".to_owned());
    }

    #[test]
    #[should_panic]
    fn test_depot_panic1() {
        let depot = Depot::new();
        depot.borrow::<String>("one");
    }
    #[test]
    #[should_panic]
    fn test_depot_panic2() {
        let mut depot = Depot::new();
        depot.take::<String>("one");
    }
}
