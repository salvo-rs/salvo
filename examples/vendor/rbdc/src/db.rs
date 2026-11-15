use crate::Error;
use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use rbs::Value;
use rbs::value::map::ValueMap;
use std::any::Any;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

/// Represents database driver that can be shared between threads, and can therefore implement
/// a connection pool
pub trait Driver: Debug + Sync + Send {
    fn name(&self) -> &str;
    /// Create a connection to the database. Note that connections are intended to be used
    /// in a single thread since most database connections are not thread-safe
    fn connect(&self, url: &str) -> BoxFuture<'_, Result<Box<dyn Connection>, Error>>;

    fn connect_opt<'a>(
        &'a self,
        opt: &'a dyn ConnectOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Connection>, Error>>;

    /// make an default option
    fn default_option(&self) -> Box<dyn ConnectOptions>;

    /// Returns the database column type name for the given rbs::Value.
    /// Each driver can override this to provide driver-specific type mapping.
    fn column_type(&self, _val: &Value) -> String {
        String::new()
    }
}

impl Driver for Box<dyn Driver> {
    fn name(&self) -> &str {
        self.deref().name()
    }

    fn connect(&self, url: &str) -> BoxFuture<'_, Result<Box<dyn Connection>, Error>> {
        self.deref().connect(url)
    }

    fn connect_opt<'a>(
        &'a self,
        opt: &'a dyn ConnectOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Connection>, Error>> {
        self.deref().connect_opt(opt)
    }

    fn default_option(&self) -> Box<dyn ConnectOptions> {
        self.deref().default_option()
    }

    fn column_type(&self, val: &Value) -> String {
        self.deref().column_type(val)
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct ExecResult {
    pub rows_affected: u64,
    /// If some databases do not support last_insert_id, the default value is Null
    pub last_insert_id: Value,
}

impl Display for ExecResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct DisplayBox<'a> {
            inner: &'a Value,
        }
        impl<'a> Debug for DisplayBox<'a> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.inner, f)
            }
        }
        f.debug_map()
            .key(&"rows_affected")
            .value(&self.rows_affected)
            .key(&"last_insert_id")
            .value(&DisplayBox {
                inner: &self.last_insert_id,
            })
            .finish()
    }
}

impl From<(u64, Value)> for ExecResult {
    fn from(value: (u64, Value)) -> Self {
        Self {
            rows_affected: value.0,
            last_insert_id: value.1,
        }
    }
}

/// Represents a connection to a database
pub trait Connection: Send + Sync {
    /// Execute a query and return results as an async stream of rows.
    /// This allows processing rows one by one without loading all results into memory.
    fn exec_rows(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<BoxStream<'_, Result<Box<dyn Row>, Error>>, Error>>;

    /// Execute a query that is expected to return a result set, such as a `SELECT` statement.
    /// you can use `let result:Vec<Table>=rbs::from_value(v)?;` to decode this result.
    /// return csv format Value [['column'],['value']].
    fn exec_decode(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<Value, Error>> {
        let v = self.exec_rows(sql, params);
        Box::pin(async move {
            use futures_util::StreamExt;
            let mut stream = v.await?;
            let mut rows = Vec::new();
            while let Some(row) = stream.next().await {
                let mut row = row?;
                let md = row.meta_data();
                let col_len = md.column_len();
                let mut m = ValueMap::with_capacity(col_len);
                for i in 0..col_len {
                    m.insert(
                        Value::String(md.column_name(i)),
                        row.get(i).unwrap_or(Value::Null),
                    );
                }
                rows.push(Value::Map(m));
            }
            Ok(Value::Array(rows))
        })
    }

    /// Execute a query that is expected to update some rows.
    fn exec(&mut self, sql: &str, params: Vec<Value>) -> BoxFuture<'_, Result<ExecResult, Error>>;

    /// ping
    fn ping(&mut self) -> BoxFuture<'_, Result<(), Error>>;

    /// close connection
    /// Normally conn is dropped when the link is dropped,
    /// but it is recommended to actively close this function so that the database does not report errors.
    /// If &mut self is not satisfied close, when you need mut self,
    /// It is recommended to use Option<DataBaseConnection>
    /// and then call take to take ownership and then if let Some(v) = self.inner.take() {v.lose ().await; }
    fn close(&mut self) -> BoxFuture<'_, Result<(), Error>>;

    /// an translation impl begin
    fn begin(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async {
            _ = self.exec("begin", vec![]).await?;
            Ok(())
        })
    }

    /// an translation impl commit
    fn commit(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async {
            _ = self.exec("commit", vec![]).await?;
            Ok(())
        })
    }

    /// an translation impl rollback
    fn rollback(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async {
            _ = self.exec("rollback", vec![]).await?;
            Ok(())
        })
    }
}

impl Connection for Box<dyn Connection> {
    fn exec_rows(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<BoxStream<'_, Result<Box<dyn Row>, Error>>, Error>> {
        self.deref_mut().exec_rows(sql, params)
    }

    fn exec_decode(
        &mut self,
        sql: &str,
        params: Vec<Value>,
    ) -> BoxFuture<'_, Result<Value, Error>> {
        self.deref_mut().exec_decode(sql, params)
    }

    fn exec(&mut self, sql: &str, params: Vec<Value>) -> BoxFuture<'_, Result<ExecResult, Error>> {
        self.deref_mut().exec(sql, params)
    }

    fn ping(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.deref_mut().ping()
    }

    fn close(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.deref_mut().close()
    }

    fn begin(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.deref_mut().begin()
    }
    fn rollback(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.deref_mut().rollback()
    }
    fn commit(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.deref_mut().commit()
    }
}

/// Result set from executing a query against a statement
pub trait Row: 'static + Send + Debug {
    /// get meta data about this result set
    fn meta_data(&self) -> Box<dyn MetaData>;

    /// get Value from index
    fn get(&mut self, i: usize) -> Result<Value, Error>;
}

/// Meta data for result set
pub trait MetaData: Debug {
    fn column_len(&self) -> usize;
    fn column_name(&self, i: usize) -> String;
    fn column_type(&self, i: usize) -> String;
}

/// connect option
pub trait ConnectOptions: Any + Send + Sync + Debug + 'static {
    /// Establish a new database connection with the options specified by `self`.
    fn connect(&self) -> BoxFuture<'_, Result<Box<dyn Connection>, Error>>;

    ///set option
    ///
    /// for exmample:
    ///
    ///```rust
    /// use std::any::Any;
    /// pub struct SqliteConnectOptions{
    ///   pub immutable:bool,
    /// };
    ///  impl SqliteConnectOptions{
    ///             pub fn new()->Self{
    ///                 Self{
    ///                     immutable: false,
    ///                 }
    ///             }
    ///             fn set(&mut self, arg: Box<dyn Any>){
    ///             }
    ///         }
    ///
    /// let mut d = SqliteConnectOptions{immutable:false};
    ///         d.set(Box::new({
    ///             let mut new = SqliteConnectOptions::new();
    ///             new.immutable=true;
    ///             new
    ///         }));
    /// ```
    ///
    #[inline]
    fn set(&mut self, arg: Box<dyn Any>)
    where
        Self: Sized,
    {
        *self = *arg.downcast().expect(
            "ConnectOptions::set: type mismatch - expected the same type that implements ConnectOptions",
        );
    }

    ///set option from uri
    fn set_uri(&mut self, uri: &str) -> Result<(), Error>;
}

/// database driver ConnectOptions
impl dyn ConnectOptions {
    pub fn downcast_ref<E: ConnectOptions>(&self) -> Option<&E> {
        <dyn Any>::downcast_ref::<E>(self)
    }

    pub fn downcast_ref_mut<E: ConnectOptions>(&mut self) -> Option<&mut E> {
        <dyn Any>::downcast_mut::<E>(self)
    }
}

/// make all database drivers support dialect '?'
/// you can use util package to impl this
/// for example:
/// ```rust
/// use rbdc::db::Placeholder;
/// pub struct MyPgDriver{}
/// impl Placeholder for MyPgDriver{
///     fn exchange(&self, sql: &str) -> String {
///         rbdc::impl_exchange("$",1,sql)
///     }
/// }
/// ```
///
/// for example: postgres driver
/// ```log
///  "select * from  table where name = ?"
/// ```
/// to
/// ```log
/// "select * from  table where name =  $1"
pub trait Placeholder {
    fn exchange(&self, sql: &str) -> String;
}
