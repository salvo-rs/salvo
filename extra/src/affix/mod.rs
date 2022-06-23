pub struct Affix(Depot);

pub fn inject<V: Any + Send + Sync>(value: V) -> Affix {
    Affix::new().inject(value)
}

pub fn insert<K, V>(key: K, value: V) -> Affix
where
    K: Into<String>,
    V: Any + Send + Sync,
{
    Affix::new().insert(value)
}

impl Affix {
    pub fn new() -> Self {
        Affix(Default::default())
    }
    pub fn inject<V: Any + Send + Sync>(self, value: V) -> Self {
        self.0.inject(value);
        self
    }

    pub fn insert<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Any + Send + Sync,
    {
        self.0.insert();
        self
    }
}

impl Default for Affix {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for Addon {
    fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        for (k, v) in self.0.inner() {
            depot.insert(k.clone(), v.clone());
        }
    }
}
