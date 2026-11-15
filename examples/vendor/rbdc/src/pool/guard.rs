use crate::db::Connection;
use crate::pool::manager::ConnectionManager;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

/// ConnectionGuard is a wrapper for a database connection make sure auto_close.
pub struct ConnectionGuard {
    pub conn: Option<Box<dyn Connection>>,
    pub manager_proxy: ConnectionManager,
    pub auto_close: Option<Duration>,
}

impl Debug for ConnectionGuard {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionBox")
            .field("manager_proxy", &self.manager_proxy)
            .field("auto_close", &self.auto_close)
            .finish()
    }
}

impl ConnectionGuard {
    /// Returns a reference to the inner connection if it hasn't been consumed.
    /// Returns `None` if the guard has already been dropped or the connection has been taken.
    pub fn get(&self) -> Option<&dyn Connection> {
        self.conn.as_deref()
    }

    /// Returns a mutable reference to the inner connection if it hasn't been consumed.
    /// Returns `None` if the guard has already been dropped or the connection has been taken.
    pub fn get_mut(&mut self) -> Option<&mut dyn Connection> {
        match &mut self.conn {
            Some(c) => Some(c.as_mut()),
            None => None,
        }
    }
}

impl Deref for ConnectionGuard {
    type Target = Box<dyn Connection>;

    fn deref(&self) -> &Self::Target {
        self.conn
            .as_ref()
            .expect("ConnectionGuard has been consumed - connection already closed or taken")
    }
}

impl DerefMut for ConnectionGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn
            .as_mut()
            .expect("ConnectionGuard has been consumed - connection already closed or taken")
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(auto_close) = self.auto_close {
            if let Some(mut conn) = self.conn.take() {
                self.manager_proxy.spawn_task(async move {
                    let _ = tokio::time::timeout(auto_close, conn.close()).await;
                });
            }
        }
    }
}
