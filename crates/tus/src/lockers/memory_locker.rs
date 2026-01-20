use std::collections::HashMap;
use std::sync::Arc;

use salvo_core::async_trait;
use tokio::sync::{Mutex, RwLock};

use crate::error::TusResult;
use crate::lockers::{LockGuard, Locker};

#[derive(Clone)]
pub struct MemoryLocker {
    inner: Arc<Mutex<HashMap<String, Arc<RwLock<()>>>>>,
}

impl MemoryLocker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn get_lock(&self, id: &str) -> Arc<RwLock<()>> {
        let mut map = self.inner.lock().await;
        map.entry(id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }
}

#[async_trait]
impl Locker for MemoryLocker {
    async fn lock(&self, id: &str) -> TusResult<LockGuard> {
        self.write_lock(id).await
    }

    async fn read_lock(&self, id: &str) -> TusResult<LockGuard> {
        let lock = self.get_lock(id).await;
        let guard = lock.read_owned().await;
        Ok(LockGuard::read(guard))
    }

    async fn write_lock(&self, id: &str) -> TusResult<LockGuard> {
        let lock = self.get_lock(id).await;
        let guard = lock.write_owned().await;
        Ok(LockGuard::write(guard))
    }
}
