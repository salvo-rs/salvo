//! Connection-owned progress for asynchronous admission.

use std::fmt::{self, Debug, Formatter};

use futures_util::future::{BoxFuture, FutureExt};

use super::ConnCtrl;
use crate::fuse::{ArcConnObserver, ArcFusePolicy, FuseAction, FuseConfig, FuseInfo, TransProto};

pub(super) struct Admitted {
    pub(super) config: Option<FuseConfig>,
    pub(super) observer: Option<ArcConnObserver>,
}

/// Retains both the transport and its decision across cancellation of `accept`.
/// The policy selected when this value is created owns admission until it finishes.
pub(super) struct PendingAdmission<T> {
    pub(super) incoming: T,
    pub(super) info: FuseInfo,
    pub(super) conn_ctrl: ConnCtrl,
    decision: Option<BoxFuture<'static, Option<Admitted>>>,
}

impl<T> PendingAdmission<T> {
    pub(super) fn new(incoming: T, info: FuseInfo, policy: Option<ArcFusePolicy>) -> Self {
        let conn_ctrl = ConnCtrl::new();
        let decision = policy.map(|policy| {
            let info = info.clone();
            let conn_ctrl = conn_ctrl.clone();
            async move {
                match policy.decide(&info).await {
                    FuseAction::Accept(config) => Some(Admitted {
                        config: Some(config),
                        // QUIC has no byte-stream observer; keep its existing behavior.
                        observer: (info.trans_proto == TransProto::Tcp)
                            .then(|| policy.observe(&info, &conn_ctrl))
                            .flatten(),
                    }),
                    FuseAction::Reject => None,
                }
            }
            .boxed()
        });
        Self {
            incoming,
            info,
            conn_ctrl,
            decision,
        }
    }

    pub(super) async fn decide(&mut self) -> Option<Admitted> {
        match &mut self.decision {
            Some(decision) => decision.await,
            None => Some(Admitted {
                config: None,
                observer: None,
            }),
        }
    }
}

impl<T> Debug for PendingAdmission<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingAdmission")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}
