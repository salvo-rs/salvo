//! Tests for `rbdc::common` helpers: `StatementCache`, `DebugFn` and `UStr`.

use rbdc::common::{DebugFn, StatementCache};
use rbdc::ext::ustr::UStr;
use std::borrow::Borrow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[test]
fn test_statement_cache_basic() {
    let mut cache: StatementCache<u32> = StatementCache::new(3);
    assert_eq!(cache.capacity(), 3);
    assert_eq!(cache.len(), 0);
    assert!(cache.is_enabled());
    assert!(!cache.contains_key("a"));

    assert_eq!(cache.insert("a", 1), None);
    assert_eq!(cache.insert("b", 2), None);
    assert!(cache.contains_key("a"));
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.get_mut("a"), Some(&mut 1));
    *cache.get_mut("a").unwrap() = 10;
    assert_eq!(cache.get_mut("a"), Some(&mut 10));

    assert_eq!(cache.remove("a"), Some(10));
    assert_eq!(cache.remove("a"), None);
    assert!(!cache.contains_key("a"));

    cache.insert("c", 3);
    cache.clear();
    assert_eq!(cache.len(), 0);
}

#[test]
fn test_statement_cache_eviction() {
    let mut cache: StatementCache<u32> = StatementCache::new(2);
    cache.insert("a", 1);
    cache.insert("b", 2);
    // Touch "a" so "b" becomes the least recently used entry.
    let _ = cache.get_mut("a");
    assert_eq!(cache.insert("c", 3), Some(2));
    assert!(cache.contains_key("a"));
    assert!(!cache.contains_key("b"));
    assert!(cache.contains_key("c"));
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_statement_cache_replace_existing() {
    let mut cache: StatementCache<u32> = StatementCache::new(2);
    cache.insert("a", 1);
    assert_eq!(cache.insert("a", 9), Some(1));
    assert_eq!(cache.get_mut("a"), Some(&mut 9));
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_statement_cache_remove_lru() {
    let mut cache: StatementCache<u32> = StatementCache::new(2);
    cache.insert("a", 1);
    cache.insert("b", 2);
    let _ = cache.get_mut("a");
    assert_eq!(cache.remove_lru(), Some(2));
    assert_eq!(cache.remove_lru(), Some(1));
    assert_eq!(cache.remove_lru(), None);
}

#[test]
#[should_panic]
fn test_statement_cache_zero_capacity_panics() {
    let _: StatementCache<u32> = StatementCache::new(0);
}

struct Counter(i32);

impl Counter {
    fn bump(&mut self) -> i32 {
        self.0 += 1;
        self.0
    }
}

#[test]
fn test_debug_fn() {
    let mut f = DebugFn(Counter(0));
    assert_eq!(format!("{:?}", f), "Function");
    // Deref and DerefMut
    assert_eq!((*f).bump(), 1);
    assert_eq!((*f).0, 1);
}

#[test]
fn test_ustr() {
    let shared = UStr::new("dynamic");
    let stat: UStr = "static".into();
    let owned: UStr = String::from("owned").into();

    assert_eq!(&*shared, "dynamic");
    assert_eq!(&*stat, "static");
    assert_eq!(&*owned, "owned");
    assert_eq!(shared, UStr::new("dynamic"));
    assert_ne!(shared, stat);
    assert_eq!(shared.to_string(), "dynamic");
    assert_eq!(format!("{:?}", stat), "static");

    // Borrow<str> makes UStr usable as a HashMap key with &str lookups.
    let borrowed: &str = stat.borrow();
    assert_eq!(borrowed, "static");

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }
    assert_eq!(hash_of(&shared), hash_of(&"dynamic"));
    assert_eq!(hash_of(&owned), hash_of(&"owned"));

    let json = serde_json::to_string(&shared).unwrap();
    assert_eq!(json, "\"dynamic\"");
    let back: UStr = serde_json::from_str(&json).unwrap();
    assert_eq!(back, shared);
}
