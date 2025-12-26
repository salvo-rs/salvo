use std::{collections::HashMap, sync::Arc};

use salvo_core::async_trait;
use tokio::sync::Mutex;

use crate::{error::TusResult, lockers::{LockGuard, Locker}};

#[derive(Clone)]
pub struct MemoryLocker {
    inner: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl MemoryLocker {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }
}

#[async_trait]
impl Locker for MemoryLocker {
    async fn lock(&self, id: &str) -> TusResult<LockGuard> {
        let m = {
            let mut map = self.inner.lock().await;
            map.entry(id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let guard = m.lock_owned().await;
        Ok(LockGuard { _guard: guard })
    }
}