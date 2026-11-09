use std::pin::Pin;

pub(super) type UpgradeTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Owns upgraded tasks even after the HTTP driver hands over the transport.
#[cfg(feature = "server")]
pub(crate) struct UpgradeScope {
    ctrl: super::ConnCtrl,
    receiver: tokio::sync::mpsc::UnboundedReceiver<UpgradeTask>,
    tasks: tokio::task::JoinSet<()>,
}

#[cfg(feature = "server")]
impl UpgradeScope {
    pub(super) fn new(
        ctrl: super::ConnCtrl,
        receiver: tokio::sync::mpsc::UnboundedReceiver<UpgradeTask>,
    ) -> Self {
        Self {
            ctrl,
            receiver,
            tasks: tokio::task::JoinSet::new(),
        }
    }

    pub(crate) async fn run<C: Future, S: Future>(mut self, connection: C, stop: S) {
        tokio::pin!(connection, stop);
        let mut http_done = false;
        loop {
            if http_done && self.receiver.is_empty() && self.tasks.is_empty() {
                return;
            }
            tokio::select! {
                biased;
                _ = &mut stop => break,
                _ = self.ctrl.aborted() => break,
                _ = &mut connection, if !http_done => {
                    http_done = true;
                    // Upgrade handlers register before returning their response.
                    // Closing now preserves queued tasks and rejects late work.
                    self.receiver.close();
                }
                Some(task) = self.receiver.recv() => {
                    self.tasks.spawn(task);
                }
                Some(result) = self.tasks.join_next(), if !self.tasks.is_empty() => {
                    if let Err(error) = result {
                        tracing::error!(?error, "upgraded connection task failed");
                    }
                }
            }
        }
        self.receiver.close();
        // Drop queued tasks as well as cancel running tasks. Wait for their
        // destructors before releasing the server's connection guard/permit.
        while self.receiver.try_recv().is_ok() {}
        self.tasks.shutdown().await;
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tokio::sync::Notify;

    use super::super::ConnCtrl;

    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn registration_on_the_final_http_poll_is_driven_until_completion() {
        let ctrl = ConnCtrl::new();
        let scope = ctrl.upgrade_scope();
        let (started, received) = tokio::sync::oneshot::channel();
        let finish = Arc::new(Notify::new());
        let finish_in_task = finish.clone();
        let connection = async move {
            ctrl.spawn_upgrade(async move {
                started.send(()).unwrap();
                finish_in_task.notified().await;
            });
        };
        let task = tokio::spawn(scope.run(connection, std::future::pending::<()>()));
        tokio::time::timeout(Duration::from_secs(1), received)
            .await
            .unwrap()
            .unwrap();
        assert!(!task.is_finished());
        finish.notify_one();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn force_stop_drops_an_upgrade_queued_before_the_scope_is_polled() {
        let ctrl = ConnCtrl::new();
        let scope = ctrl.upgrade_scope();
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        ctrl.spawn_upgrade(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        scope.run(std::future::pending::<()>(), async {}).await;
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn dropping_the_connection_scope_cancels_its_parked_upgrade() {
        let ctrl = ConnCtrl::new();
        let scope = ctrl.upgrade_scope();
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let started = Arc::new(Notify::new());
        let started_in_task = started.clone();
        ctrl.spawn_upgrade(async move {
            let _guard = guard;
            started_in_task.notify_one();
            std::future::pending::<()>().await;
        });
        let task = tokio::spawn(scope.run(async {}, std::future::pending::<()>()));
        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(1), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a dropped scope must cancel its upgrade task");
    }

    #[tokio::test]
    async fn a_panicking_upgrade_does_not_stall_connection_drain() {
        let ctrl = ConnCtrl::new();
        let scope = ctrl.upgrade_scope();
        ctrl.spawn_upgrade(async { panic!("intentional upgrade callback panic") });
        tokio::time::timeout(
            Duration::from_secs(1),
            scope.run(async {}, std::future::pending::<()>()),
        )
        .await
        .expect("a panicked upgrade must release its connection");
    }

    #[tokio::test]
    async fn registering_after_the_connection_ends_does_not_detach_a_task() {
        let ctrl = ConnCtrl::new();
        let scope = ctrl.upgrade_scope();
        scope.run(async {}, std::future::pending::<()>()).await;
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        ctrl.spawn_upgrade(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        assert!(dropped.load(Ordering::SeqCst));
    }
}
