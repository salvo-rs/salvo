use std::borrow::Cow;

use lru::LruCache;
use postgres_types::Type;
use tokio_postgres::Client;
use tokio_postgres::{Error, Statement};

#[derive(Debug, Clone)]
pub struct StatementCache {
    inner: LruCache<Key<'static>, Statement>,
}

impl StatementCache {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: LruCache::new(capacity.try_into().unwrap()),
        }
    }

    pub fn get(&mut self, query: &str, types: &[Type]) -> Option<Statement> {
        self.inner
            .get(&Key::new(query, types).into_owned())
            .map(ToOwned::to_owned)
    }

    pub fn insert(&mut self, query: &str, types: &[Type], statement: Statement) {
        self.inner
            .put(Key::new(query, types).into_owned(), statement);
    }

    pub async fn prepare_typed(
        &mut self,
        client: &mut Client,
        query: &str,
        types: &[Type],
    ) -> Result<Statement, Error> {
        if let Some(statement) = self.get(query, types) {
            Ok(statement)
        } else {
            let stmt = client.prepare_typed(query, types).await?;
            self.insert(query, types, stmt.clone());
            Ok(stmt)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key<'a> {
    query: Cow<'a, str>,
    types: Cow<'a, [Type]>,
}

impl<'a> Key<'a> {
    #[must_use]
    pub fn new(query: &'a str, types: &'a [Type]) -> Self {
        Self {
            query: query.into(),
            types: types.into(),
        }
    }

    pub fn into_owned(self) -> Key<'static> {
        Key::<'static> {
            query: self.query.into_owned().into(),
            types: self.types.into_owned().into(),
        }
    }
}
