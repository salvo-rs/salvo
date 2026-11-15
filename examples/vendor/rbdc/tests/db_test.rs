//! Tests for the `rbdc::db` traits and their blanket/default implementations.

use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use futures_util::stream;
use rbdc::db::{ConnectOptions, Connection, Driver, ExecResult, MetaData, Placeholder, Row};
use rbdc::Error;
use rbs::value::map::ValueMap;
use rbs::Value;

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct MockOptions {
    uri: String,
}

impl ConnectOptions for MockOptions {
    fn connect(&self) -> BoxFuture<'_, Result<Box<dyn Connection>, Error>> {
        Box::pin(async { Ok(Box::new(MockConn::default()) as Box<dyn Connection>) })
    }

    fn set_uri(&mut self, uri: &str) -> Result<(), Error> {
        self.uri = uri.to_string();
        Ok(())
    }
}

#[derive(Debug)]
struct MockDriver;

impl Driver for MockDriver {
    fn name(&self) -> &str {
        "mock"
    }

    fn connect(&self, _url: &str) -> BoxFuture<'_, Result<Box<dyn Connection>, Error>> {
        Box::pin(async { Ok(Box::new(MockConn::default()) as Box<dyn Connection>) })
    }

    fn connect_opt<'a>(
        &'a self,
        _opt: &'a dyn ConnectOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Connection>, Error>> {
        Box::pin(async { Ok(Box::new(MockConn::default()) as Box<dyn Connection>) })
    }

    fn default_option(&self) -> Box<dyn ConnectOptions> {
        Box::new(MockOptions::default())
    }
}

#[derive(Debug, Clone)]
struct MockRow {
    cols: Vec<String>,
    values: Vec<Value>,
}

impl MockRow {
    fn new(cols: Vec<&str>, values: Vec<Value>) -> Self {
        Self {
            cols: cols.into_iter().map(str::to_string).collect(),
            values,
        }
    }
}

#[derive(Debug)]
struct MockMetaData {
    cols: Vec<String>,
}

impl MetaData for MockMetaData {
    fn column_len(&self) -> usize {
        self.cols.len()
    }

    fn column_name(&self, i: usize) -> String {
        self.cols.get(i).cloned().unwrap_or_default()
    }

    fn column_type(&self, _i: usize) -> String {
        "TEXT".to_string()
    }
}

impl Row for MockRow {
    fn meta_data(&self) -> Box<dyn MetaData> {
        Box::new(MockMetaData {
            cols: self.cols.clone(),
        })
    }

    fn get(&mut self, i: usize) -> Result<Value, Error> {
        self.values
            .get(i)
            .cloned()
            .ok_or_else(|| Error::from("index out of bounds"))
    }
}

#[derive(Debug, Default)]
struct MockConn {
    rows: Vec<MockRow>,
    calls: Vec<String>,
    ping_error: bool,
}

impl Connection for MockConn {
    fn exec_rows(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<BoxStream<'_, Result<Box<dyn Row>, Error>>, Error>> {
        self.calls.push(format!("exec_rows:{sql}:{}", params.len()));
        let rows: Vec<Result<Box<dyn Row>, Error>> = self
            .rows
            .iter()
            .cloned()
            .map(|r| Ok(Box::new(r) as Box<dyn Row>))
            .collect();
        Box::pin(async move {
            let stream: BoxStream<'static, Result<Box<dyn Row>, Error>> =
                Box::pin(stream::iter(rows));
            Ok(stream)
        })
    }

    fn exec(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<ExecResult, Error>> {
        self.calls.push(format!("exec:{sql}:{}", params.len()));
        Box::pin(async move {
            Ok(ExecResult {
                rows_affected: 1,
                last_insert_id: Value::I64(2),
            })
        })
    }

    fn ping(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.calls.push("ping".to_string());
        Box::pin(async move {
            if self.ping_error {
                Err(Error::from("ping failed"))
            } else {
                Ok(())
            }
        })
    }

    fn close(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.calls.push("close".to_string());
        Box::pin(async { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// ExecResult
// ---------------------------------------------------------------------------

#[test]
fn test_exec_result() {
    let r = ExecResult::from((3u64, Value::U64(7)));
    assert_eq!(r.rows_affected, 3);
    assert_eq!(r.last_insert_id, Value::U64(7));

    let displayed = format!("{r}");
    assert!(displayed.contains("rows_affected"));
    assert!(displayed.contains("3"));
    assert!(displayed.contains("last_insert_id"));
    assert!(displayed.contains("7"));

    let default = ExecResult::default();
    assert_eq!(default.rows_affected, 0);
    assert_eq!(default.last_insert_id, Value::Null);

    // serde round trip
    let json = serde_json::to_value(&r).unwrap();
    let back: ExecResult = serde_json::from_value(json).unwrap();
    assert_eq!(back, r);
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_driver_and_boxed_driver() {
    let driver = MockDriver;
    assert_eq!(driver.name(), "mock");
    assert_eq!(driver.column_type(&Value::Null), "");
    assert_eq!(driver.default_option().downcast_ref::<MockOptions>().unwrap().uri, "");

    let mut conn = driver.connect("mock://url").await.unwrap();
    assert!(conn.ping().await.is_ok());

    let opt = MockOptions::default();
    let _ = driver.connect_opt(&opt).await.unwrap();

    // Blanket impl for Box<dyn Driver> forwards every method.
    let boxed: Box<dyn Driver> = Box::new(MockDriver);
    assert_eq!(boxed.name(), "mock");
    assert_eq!(boxed.column_type(&Value::I64(1)), "");
    let _ = boxed.default_option();
    let _ = boxed.connect("mock://").await.unwrap();
    let _ = boxed.connect_opt(&opt).await.unwrap();
}

// ---------------------------------------------------------------------------
// ConnectOptions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_connect_options_set_and_downcast() {
    let mut opt = MockOptions::default();
    opt.set_uri("mock://a").unwrap();
    assert_eq!(opt.uri, "mock://a");

    // The provided `set` replaces the whole value (or panics on type mismatch).
    opt.set(Box::new(MockOptions {
        uri: "mock://b".to_string(),
    }));
    assert_eq!(opt.uri, "mock://b");

    let dyn_opt: &dyn ConnectOptions = &opt;
    assert_eq!(dyn_opt.downcast_ref::<MockOptions>().unwrap().uri, "mock://b");

    {
        let dyn_opt: &mut dyn ConnectOptions = &mut opt;
        dyn_opt.downcast_ref_mut::<MockOptions>().unwrap().uri = "mock://c".to_string();
        let _ = dyn_opt.connect().await.unwrap();
    }
    assert_eq!(opt.uri, "mock://c");
}

// ---------------------------------------------------------------------------
// Connection default methods and boxed forwarding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_connection_default_transaction_methods() {
    let mut conn = MockConn::default();
    conn.begin().await.unwrap();
    conn.commit().await.unwrap();
    conn.rollback().await.unwrap();
    assert_eq!(
        conn.calls,
        vec!["exec:begin:0", "exec:commit:0", "exec:rollback:0"]
    );
}

#[tokio::test]
async fn test_exec_decode_default_impl() {
    let mut conn = MockConn {
        rows: vec![MockRow::new(
            vec!["a", "b"],
            // "b" is missing on purpose: the default impl maps it to Null.
            vec![Value::I64(1)],
        )],
        ..Default::default()
    };

    let decoded = conn.exec_decode("select", vec![Value::Null]).await.unwrap();
    let mut expected = ValueMap::new();
    expected.insert(Value::String("a".into()), Value::I64(1));
    expected.insert(Value::String("b".into()), Value::Null);
    assert_eq!(decoded, Value::Array(vec![Value::Map(expected)]));
    assert_eq!(conn.calls, vec!["exec_rows:select:1"]);
}

#[tokio::test]
async fn test_boxed_connection_forwards() {
    let mut conn: Box<dyn Connection> = Box::new(MockConn {
        rows: vec![MockRow::new(vec!["a"], vec![Value::I64(1)])],
        ..Default::default()
    });

    let rows = conn.exec_rows("s", vec![]).await.unwrap();
    drop(rows);
    let _ = conn.exec("s", vec![]).await.unwrap();
    let _ = conn.exec_decode("s", vec![]).await.unwrap();
    assert!(conn.ping().await.is_ok());
    conn.begin().await.unwrap();
    conn.commit().await.unwrap();
    conn.rollback().await.unwrap();
    conn.close().await.unwrap();
}

#[tokio::test]
async fn test_connection_ping_error() {
    let mut conn = MockConn {
        ping_error: true,
        ..Default::default()
    };
    assert!(conn.ping().await.is_err());
}

// ---------------------------------------------------------------------------
// Placeholder
// ---------------------------------------------------------------------------

struct PgLikePlaceholder;

impl Placeholder for PgLikePlaceholder {
    fn exchange(&self, sql: &str) -> String {
        rbdc::impl_exchange("$", 1, sql)
    }
}

#[test]
fn test_placeholder() {
    let p = PgLikePlaceholder;
    assert_eq!(p.exchange("select * from t where a = ?"), "select * from t where a = $1");
}
