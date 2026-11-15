//! Process-local storage used when persistence was not requested.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use certon::error::StorageError;
use certon::storage::{KeyInfo, Storage};
use salvo_core::async_trait;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Default)]
pub(crate) struct MemoryStorage {
    values: Mutex<HashMap<String, (Vec<u8>, KeyInfo)>>,
    locks: Mutex<HashMap<String, Arc<Semaphore>>>,
    held: Mutex<HashMap<String, OwnedSemaphorePermit>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn is_child(key: &str, parent: &str) -> bool {
    parent.is_empty()
        || key
            .strip_prefix(parent)
            .is_some_and(|tail| tail.starts_with('/'))
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn store(&self, key: &str, value: &[u8]) -> certon::Result<()> {
        lock(&self.values).insert(
            key.into(),
            (
                value.to_vec(),
                KeyInfo {
                    key: key.into(),
                    modified: SystemTime::now().into(),
                    size: value.len() as u64,
                    is_terminal: true,
                },
            ),
        );
        Ok(())
    }

    async fn load(&self, key: &str) -> certon::Result<Vec<u8>> {
        lock(&self.values)
            .get(key)
            .map(|(value, _)| value.clone())
            .ok_or_else(|| StorageError::NotFound(key.into()).into())
    }

    async fn delete(&self, key: &str) -> certon::Result<()> {
        lock(&self.values).retain(|name, _| name != key && !is_child(name, key));
        Ok(())
    }

    async fn exists(&self, key: &str) -> certon::Result<bool> {
        Ok(lock(&self.values)
            .keys()
            .any(|name| name == key || is_child(name, key)))
    }

    async fn list(&self, path: &str, recursive: bool) -> certon::Result<Vec<String>> {
        let mut names = BTreeSet::new();
        for key in lock(&self.values).keys().filter(|key| is_child(key, path)) {
            let tail = if path.is_empty() {
                key.as_str()
            } else {
                &key[path.len() + 1..]
            };
            let mut prefix = path.to_owned();
            for component in tail.split('/') {
                if prefix.is_empty() {
                    prefix.push_str(component);
                } else {
                    prefix.push('/');
                    prefix.push_str(component);
                }
                names.insert(prefix.clone());
                if !recursive {
                    break;
                }
            }
        }
        Ok(names.into_iter().collect())
    }

    async fn stat(&self, key: &str) -> certon::Result<KeyInfo> {
        let values = lock(&self.values);
        if let Some((_, info)) = values.get(key) {
            return Ok(info.clone());
        }
        if values.keys().any(|name| is_child(name, key)) {
            return Ok(KeyInfo {
                key: key.into(),
                modified: SystemTime::now().into(),
                size: 0,
                is_terminal: false,
            });
        }
        Err(StorageError::NotFound(key.into()).into())
    }

    async fn lock(&self, name: &str) -> certon::Result<()> {
        let semaphore = lock(&self.locks)
            .entry(name.into())
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone();
        let permit = semaphore
            .acquire_owned()
            .await
            .map_err(|error| certon::Error::Other(error.to_string()))?;
        lock(&self.held).insert(name.into(), permit);
        Ok(())
    }

    async fn unlock(&self, name: &str) -> certon::Result<()> {
        lock(&self.held).remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_are_atomic_and_prefix_operations_are_component_aware() {
        let storage = MemoryStorage::default();
        storage.store("certs/a/key", b"secret").await.unwrap();
        storage.store("certs/ab/key", b"other").await.unwrap();
        assert_eq!(storage.load("certs/a/key").await.unwrap(), b"secret");
        assert_eq!(
            storage.list("certs", false).await.unwrap(),
            ["certs/a", "certs/ab"]
        );
        assert_eq!(
            storage.list("certs", true).await.unwrap(),
            ["certs/a", "certs/a/key", "certs/ab", "certs/ab/key"]
        );
        assert!(!storage.stat("certs/a").await.unwrap().is_terminal);
        storage.delete("certs/a").await.unwrap();
        assert!(!storage.exists("certs/a").await.unwrap());
        assert!(storage.exists("certs/ab/key").await.unwrap());
    }

    #[tokio::test]
    async fn lock_waiters_do_not_enter_until_unlock() {
        let storage = MemoryStorage::default();
        storage.lock("one").await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), storage.lock("one"))
                .await
                .is_err()
        );
        storage.unlock("one").await.unwrap();
        storage.lock("one").await.unwrap();
        storage.unlock("one").await.unwrap();
    }
}
