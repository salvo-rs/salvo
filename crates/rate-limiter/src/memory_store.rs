//TODO

use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;

use salvo_core::async_trait;
use tokio::sync::Mutex;

use super::{RateGuard, RateStore};

/// A simple in-memory store for rate limiter.
#[derive(Default, Debug)]
pub struct MemoryStore<K, E> {
    inner: Mutex<HashMap<K, E>>,
}
impl<K, E> MemoryStore<K, E>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    /// Create a new `MemoryStore`.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl<K, G> RateStore for MemoryStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
    G: RateGuard,
{
    type Error = Infallible;
    type Key = K;
    type Guard = G;

    async fn load_guard<Q>(&self, key: &Q, refer: &Self::Guard) -> Result<Self::Guard, Self::Error>
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        let mut inner = self.inner.lock().await;
        let guard = inner.remove(key);
        if let Some(guard) = guard {
            Ok(guard)
        } else {
            Ok(refer.clone())
        }
    }

    async fn save_guard(&self, key: Self::Key, guard: Self::Guard) -> Result<(), Self::Error> {
        let mut inner = self.inner.lock().await;
        inner.insert(key, guard);
        Ok(())
    }
}
