
use std::sync::Arc;
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use futures_util::future;
use async_trait::async_trait;

use futures::future::FutureExt;

use crate::{ServerConfig, Depot};
use crate::http::{Request, Response};

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response);
}

// #[async_trait]
// impl<'a, F, R> Handler for F
// where
//     F: Send + Sync + 'static + Fn(Arc<ServerConfig>, &'a mut Request, &'a mut Depot, &'a mut Response) -> R,
//     R: Send + 'static + Future<Output=()> + 'a
// {
//     async fn handle(&self, sconf: Arc<ServerConfig>, req: &'a mut Request, depot: &'a mut Depot, resp: &'a mut Response) {
//         (*self)(sconf, req, depot, resp).await
//     }
//     // fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) -> Pin<Box<dyn Future<Output = User> + Send + '_>>  {
//     //     Box::pin(async move {(*self)(sconf, req, depot, resp).await})
//     // }
// }
// #[async_trait]
// impl<T, Y> Handler for (T, Y) where T: Handler, Y: Handler
//         {
//             async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
             
//             }
//         }
macro_rules! handler_tuple_impls {
    ($(
        $Tuple:tt {
            $(($idx:tt) -> $T:tt,)+ //https://github.com/dtolnay/async-trait/issues/46
        }
    )+) => {$(
        #[async_trait]
        impl<$($T,)+> Handler for ($($T,)+) where $($T: Handler,)+
        {
            async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
                $(
                    if !resp.is_commited() {
                        self.$idx.handle(sconf.clone(), req, depot, resp).await;
                    } else {
                        return;
                    }
                )+
            }
        })+
    }
}
#[doc(hidden)]
macro_rules! __for_each_handler_tuple {
    ($callback:ident) => {
        $callback! {
            1 {
                (0) -> A,
            }
            2 {
                (0) -> A,
                (1) -> B,
            }
            3 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
            }
            4 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
            }
            5 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
            }
            6 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
            }
            7 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
            }
            8 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
            }
            9 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
            }
            10 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
            }
            11 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
                (10) -> K,
            }
            12 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
                (10) -> K,
                (11) -> L,
            }
            13 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
                (10) -> K,
                (11) -> L,
                (12) -> M,
            }
            14 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
                (10) -> K,
                (11) -> L,
                (12) -> M,
                (13) -> N,
            }
            15 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
                (10) -> K,
                (11) -> L,
                (12) -> M,
                (13) -> N,
                (14) -> O,
            }
            16 {
                (0) -> A,
                (1) -> B,
                (2) -> C,
                (3) -> D,
                (4) -> E,
                (5) -> F,
                (6) -> G,
                (7) -> H,
                (8) -> I,
                (9) -> J,
                (10) -> K,
                (11) -> L,
                (12) -> M,
                (13) -> N,
                (14) -> O,
                (15) -> P,
            }
        }
    };
}

__for_each_handler_tuple!(handler_tuple_impls);