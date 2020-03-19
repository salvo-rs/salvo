use std::any::Any;
use std::collections::HashMap;

pub struct Depot {
    data: HashMap<String, Box<dyn Any + Send>>,
}

impl Depot {
    pub(crate) fn new() -> Depot {
        Depot { data: HashMap::new() }
    }

    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.insert(key.into(), Box::new(value));
    }

    pub fn has<K>(&self, key: K) -> bool
    where
        K: Into<String>,
    {
        self.data.get(&key.into()).is_some()
    }

    pub fn try_borrow<K, V>(&self, key: K) -> Option<&V>
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.get(&key.into()).and_then(|b| b.downcast_ref::<V>())
    }

    pub fn borrow<K, V>(&self, key: K) -> &V
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.try_borrow(key).expect("required type is not present in depot container")
    }
    pub fn try_borrow_mut<K, V>(&mut self, key: K) -> Option<&mut V>
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.get_mut(&key.into()).and_then(|b| b.downcast_mut::<V>())
    }

    pub fn borrow_mut<K, V>(&mut self, key: K) -> &mut V
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.try_borrow_mut(key)
            .expect("required type is not present in depot container")
    }

    pub fn try_take<K, V>(&mut self, key: K) -> Option<V>
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.data.remove(&key.into()).and_then(|b| b.downcast::<V>().ok()).map(|b| *b)
    }

    pub fn take<K, V>(&mut self, key: K) -> V
    where
        K: Into<String>,
        V: Any + Send,
    {
        self.try_take(key).expect("required type is not present in depot container")
    }
}
