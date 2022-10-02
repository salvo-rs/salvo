//TODO

use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;

use salvo_core::{async_trait, Handler};
use tokio::sync::Mutex;

use super::{RateStore, RateStrategy};

#[derive(Default, Debug)]
pub struct MemoryStore<K, G> {
    inner: Mutex<HashMap<K, G>>,
}
impl<K, G> MemoryStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
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
    G: RateStrategy,
{
    type Error = Infallible;
    type Key = K;
    type Strategy = G;

    async fn load_strategy(&self, key: &Self::Key, config: &Self::Strategy) -> Result<Self::Strategy, Self::Error> {
        let mut inner = self.inner.lock().await;
        let data = inner.remove(key);
        if let Some(data) = data {
            Ok(data)
        } else {
            Ok(config.clone())
        }
    }

    async fn save_strategy(&self, key: Self::Key, strategy: Self::Strategy) -> Result<(), Self::Error> {
        let mut inner = self.inner.lock().await;
        inner.insert(key, strategy);
        Ok(())
    }
}
