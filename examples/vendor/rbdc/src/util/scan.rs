use crate::Error;
use crate::db::Row;
use futures_util::Stream;
use rbs::Value;
use rbs::value::map::ValueMap;
use std::fmt::{Debug, Formatter};
use std::pin::Pin;

/// Async Stream wrapper for scanning query results row by row.
/// More memory efficient than loading all rows into Value Array.
///
/// # Example
///
/// ```
/// use futures_util::StreamExt;
/// use rbdc::db::Connection;
/// use rbdc::util::Scan;
/// use rbdc::Error;
///
/// # async fn example(conn: &mut dyn Connection) -> Result<(), Error> {
/// let stream = conn.exec_rows("SELECT * FROM activity", vec![]).await?;
/// let scan = Scan::new(stream);
///
/// // Collect all rows into a Vec of struct
/// #[derive(serde::Deserialize)]
/// struct Activity {
///     id: Option<String>,
///     name: Option<String>,
/// }
/// let activities: Vec<Activity> = scan.collect().await?;
/// # Ok(())
/// # }
/// ```
pub struct Scan<S> {
    stream: S,
}

impl<S> Debug for Scan<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scan").finish()
    }
}

impl<S> Scan<S> {
    /// Create a new Scan from a Stream.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }
}

impl<S> Scan<S>
where
    S: Stream<Item = Result<Box<dyn Row>, Error>> + Unpin + Send,
{
    /// Collect all rows into a Vec of type T.
    /// Each row is converted from Value::Map to T using rbs::from_value.
    pub async fn collect<T: serde::de::DeserializeOwned>(mut self) -> Result<Vec<T>, Error> {
        use futures_util::StreamExt;
        let mut result = Vec::new();
        while let Some(row) = self.stream.next().await {
            let mut row = row?;
            let md = row.meta_data();
            let col_len = md.column_len();
            let mut map = ValueMap::new();
            for i in 0..col_len {
                let name = md.column_name(i);
                let value = row.get(i).unwrap_or(Value::Null);
                map.insert(Value::String(name), value);
            }
            let t: T = rbs::from_value(Value::Map(map))?;
            result.push(t);
        }
        Ok(result)
    }
}

impl<S> Stream for Scan<S>
where
    S: Stream<Item = Result<Box<dyn Row>, Error>> + Unpin + Send,
{
    type Item = Result<Value, Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // Pin the stream inside
        let stream = Pin::new(&mut self.stream);
        futures_util::pin_mut!(stream);
        stream.poll_next(cx).map(|opt| {
            opt.map(|result| {
                let mut row = result?;
                let md = row.meta_data();
                let col_len = md.column_len();
                let mut map = ValueMap::new();
                for i in 0..col_len {
                    let name = md.column_name(i);
                    let value = row.get(i).unwrap_or(Value::Null);
                    map.insert(Value::String(name), value);
                }
                Ok(Value::Map(map))
            })
        })
    }
}
