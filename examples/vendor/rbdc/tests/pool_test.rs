//! Tests for `rbdc::pool` (ConnectionManager, ConnectionGuard, Pool defaults).

use async_trait::async_trait;
use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use futures_util::stream;
use rbdc::db::{ConnectOptions, Connection, Driver, ExecResult, Row};
use rbdc::pool::{ConnectionManager, Pool};
use rbdc::Error;
use rbs::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
struct Flag {
    closed: Arc<AtomicBool>,
    pings: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct MockConn {
    flag: Flag,
}

impl Connection for MockConn {
    fn exec_rows(
        &mut self,
        _sql: &str,
        _params: Vec<Value>,
    ) -> BoxFuture<'_, Result<BoxStream<'_, Result<Box<dyn Row>, Error>>, Error>> {
        Box::pin(async {
            let stream: BoxStream<'static, Result<Box<dyn Row>, Error>> =
                Box::pin(stream::iter(Vec::new()));
            Ok(stream)
        })
    }

    fn exec(
        &mut self,
        _sql: &str,
        _params: Vec<Value>,
    ) -> BoxFuture<'_, Result<ExecResult, Error>> {
        Box::pin(async { Ok(ExecResult::default()) })
    }

    fn ping(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.flag.pings.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn close(&mut self) -> BoxFuture<'_, Result<(), Error>> {
        self.flag.closed.store(true, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

#[derive(Debug)]
struct MockOptions {
    flag: Flag,
    uri: String,
}

impl ConnectOptions for MockOptions {
    fn connect(&self) -> BoxFuture<'_, Result<Box<dyn Connection>, Error>> {
        let flag = self.flag.clone();
        Box::pin(async move { Ok(Box::new(MockConn { flag }) as Box<dyn Connection>) })
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
        Box::pin(async { Ok(Box::new(MockConn { flag: Flag::default() }) as Box<dyn Connection>) })
    }

    fn connect_opt<'a>(
        &'a self,
        opt: &'a dyn ConnectOptions,
    ) -> BoxFuture<'a, Result<Box<dyn Connection>, Error>> {
        let flag = opt
            .downcast_ref::<MockOptions>()
            .map(|o| o.flag.clone())
            .unwrap_or_default();
        Box::pin(async move { Ok(Box::new(MockConn { flag }) as Box<dyn Connection>) })
    }

    fn default_option(&self) -> Box<dyn ConnectOptions> {
        Box::new(MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        })
    }
}

#[tokio::test]
async fn test_manager_new_and_accessors() {
    let manager = ConnectionManager::new(MockDriver, "mock://uri").unwrap();
    assert_eq!(manager.driver_type(), "mock");
    assert_eq!(manager.driver_ref().name(), "mock");
    assert_eq!(manager.driver().name(), "mock");

    let option = manager.option.downcast_ref::<MockOptions>().unwrap();
    assert_eq!(option.uri, "mock://uri");
}

#[tokio::test]
async fn test_manager_connect_check_and_auto_close() {
    let flag = Flag::default();
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: flag.clone(),
            uri: String::new(),
        },
    );

    let mut guard = manager.connect().await.unwrap();
    assert!(guard.get().is_some());
    assert!(guard.get_mut().is_some());
    assert!(format!("{guard:?}").contains("ConnectionBox"));

    manager.check(&mut guard).await.unwrap();
    assert_eq!(flag.pings.load(Ordering::SeqCst), 1);

    assert!(!flag.closed.load(Ordering::SeqCst));
    drop(guard);
    // Drop schedules `close()` on the Tokio runtime.
    for _ in 0..100 {
        if flag.closed.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(flag.closed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_guard_without_auto_close_does_not_close() {
    let flag = Flag::default();
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: flag.clone(),
            uri: String::new(),
        },
    );
    let mut guard = manager.connect().await.unwrap();
    guard.auto_close = None;
    drop(guard);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!flag.closed.load(Ordering::SeqCst));
}

#[tokio::test]
#[should_panic(expected = "consumed")]
async fn test_guard_deref_after_consume_panics() {
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        },
    );
    let mut guard = manager.connect().await.unwrap();
    guard.conn = None;
    guard.auto_close = None;
    let _ = &*guard;
}

#[tokio::test]
#[should_panic(expected = "consumed")]
async fn test_guard_deref_mut_after_consume_panics() {
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        },
    );
    let mut guard = manager.connect().await.unwrap();
    guard.conn = None;
    guard.auto_close = None;
    let _ = &mut *guard;
}

#[tokio::test]
async fn test_guard_get_after_consume_is_none() {
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        },
    );
    let mut guard = manager.connect().await.unwrap();
    guard.conn = None;
    guard.auto_close = None;
    assert!(guard.get().is_none());
    assert!(guard.get_mut().is_none());
}

#[tokio::test]
async fn test_spawn_task_inside_runtime() {
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        },
    );
    let flag = Arc::new(AtomicBool::new(false));
    let flag2 = flag.clone();
    manager.spawn_task(async move {
        flag2.store(true, Ordering::SeqCst);
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn test_spawn_task_outside_runtime_is_noop() {
    let manager = ConnectionManager::new_options(
        MockDriver,
        MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        },
    );
    // Must not panic even though there is no reactor.
    manager.spawn_task(async {});
}

// ---------------------------------------------------------------------------
// Pool trait default methods
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct MockPool {
    manager: ConnectionManager,
    max_idle: AtomicUsize,
    max_open: AtomicUsize,
}

#[async_trait]
impl Pool for MockPool {
    fn new(manager: ConnectionManager) -> Result<Self, Error> {
        Ok(Self {
            manager,
            max_idle: AtomicUsize::new(0),
            max_open: AtomicUsize::new(0),
        })
    }

    async fn get(&self) -> Result<Box<dyn Connection>, Error> {
        Ok(Box::new(MockConn {
            flag: Flag::default(),
        }))
    }

    async fn get_timeout(&self, _d: Duration) -> Result<Box<dyn Connection>, Error> {
        self.get().await
    }

    async fn set_max_idle_conns(&self, n: u64) {
        self.max_idle.store(n as usize, Ordering::SeqCst);
    }

    async fn set_max_open_conns(&self, n: u64) {
        self.max_open.store(n as usize, Ordering::SeqCst);
    }

    fn driver_type(&self) -> &str {
        self.manager.driver_type()
    }

    fn driver(&self) -> &dyn Driver {
        self.manager.driver_ref()
    }
}

#[tokio::test]
async fn test_pool_trait_defaults() {
    let manager =
        ConnectionManager::new_options(MockDriver, MockOptions {
            flag: Flag::default(),
            uri: String::new(),
        });
    let pool = MockPool::new(manager).unwrap();

    assert_eq!(pool.driver_type(), "mock");
    assert_eq!(pool.driver().name(), "mock");
    pool.set_max_idle_conns(4).await;
    pool.set_max_open_conns(8).await;
    assert_eq!(pool.max_idle.load(Ordering::SeqCst), 4);
    assert_eq!(pool.max_open.load(Ordering::SeqCst), 8);

    // Default trait methods.
    pool.set_timeout(Some(Duration::from_secs(1))).await;
    pool.set_conn_max_lifetime(Some(Duration::from_secs(2))).await;
    assert_eq!(pool.state().await, Value::Null);

    let _conn = pool.get().await.unwrap();
    let _ = pool.get_timeout(Duration::from_secs(1)).await.unwrap();
}
