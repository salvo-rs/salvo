mod guard;
mod manager;
pub use guard::ConnectionGuard;
pub use manager::ConnectionManager;

use crate::{Driver, Error};
use crate::db::Connection;
use async_trait::async_trait;
use rbs::Value;
use std::fmt::Debug;
use std::time::Duration;

#[async_trait]
pub trait Pool: Sync + Send + Debug {
    /// create an Pool,use ConnManager
    fn new(manager: ConnectionManager) -> Result<Self, Error>
    where
        Self: Sized;

    /// get an connection from pool
    async fn get(&self) -> Result<Box<dyn Connection>, Error>;

    /// get timeout from pool
    async fn get_timeout(&self, d: Duration) -> Result<Box<dyn Connection>, Error>;

    async fn set_timeout(&self, _timeout: Option<Duration>) {}

    async fn set_conn_max_lifetime(&self, _max_lifetime: Option<Duration>) {}

    async fn set_max_idle_conns(&self, n: u64);

    async fn set_max_open_conns(&self, n: u64);

    ///return state
    async fn state(&self) -> Value {
        Value::Null
    }

    /// get driver_type from manager: ConnManager
    fn driver_type(&self) -> &str;

    /// driver
    fn driver(&self)-> &dyn Driver;
}
