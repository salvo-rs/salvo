//! affix middleware is used to add any data to depot.

use std::any::TypeId;

use salvo_core::async_trait;
use salvo_core::prelude::*;

trait Affix {
    fn attach(&self, depot: &mut Depot);
}

#[derive(Default)]
struct AffixCell<V> {
    key: String,
    value: V,
}
impl<T> Affix for AffixCell<T>
where
    T: Send + Sync + Clone + 'static,
{
    fn attach(&self, depot: &mut Depot) {
        depot.insert(self.key.clone(), self.value.clone());
    }
}

/// Inject a value into depot.
#[inline]
pub fn inject<V: Send + Sync + Clone + 'static>(value: V) -> AffixList {
    insert(format!("{:?}", TypeId::of::<V>()), value)
}

/// Insert a key-value pair into depot.
#[inline]
pub fn insert<K, V>(key: K, value: V) -> AffixList
where
    K: Into<String>,
    V: Send + Sync + Clone + 'static,
{
    AffixList::new().insert(key, value)
}

/// AffixList is used to add any data to depot.
#[derive(Default)]
pub struct AffixList(Vec<Box<dyn Affix + Send + Sync + 'static>>);
impl AffixList {
    /// Create an empty affix list.
    pub fn new() -> AffixList {
        AffixList(Vec::new())
    }
    /// Inject a value into depot.
    pub fn inject<V: Send + Sync + Clone + 'static>(self, value: V) -> Self {
        self.insert(format!("{:?}", TypeId::of::<V>()), value)
    }

    /// Insert a key-value pair into depot.
    pub fn insert<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Send + Sync + Clone + 'static,
    {
        let cell = AffixCell { key: key.into(), value };
        self.0.push(Box::new(cell));
        self
    }
}

#[async_trait]
impl Handler for AffixList {
    async fn handle(&self, _req: &mut Request, depot: &mut Depot, _res: &mut Response, _ctrl: &mut FlowCtrl) {
        for cell in &self.0 {
            cell.attach(depot);
        }
    }
}
