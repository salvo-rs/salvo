
use std::sync::Arc;
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;
use futures_util::future;
use async_trait::async_trait;

use crate::{ServerConfig, Depot};
use crate::http::{Request, Response};

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response);
}

#[async_trait]
impl<F, R> Handler for F
where
    R: Send + 'static + Future<Output=()>,
    F: Send + Sync + 'static + FnMut(Arc<ServerConfig>, &mut Request, &mut Depot, &mut Response) -> R
{
    async fn handle(&self, sconf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
        (*self)(sconf, req, depot, resp).await;
    }
}
//https://github.com/rust-lang/rust/issues/60074
// impl<F> Handler for F
// where
//     F: Send + Sync + 'static + Fn(&mut Context) -> HttpResult<Box<dyn Content>>,
// {
//     fn handle(&self, ctx: &mut Context) {
//         match (*self)(ctx) {
//             Ok(content) => {
//                 ctx.write_content(content);
//             },
//             Err(err) => {
//                 ctx.write_error(err);
//             },
//         }
//     }
// }

// macro_rules! handler_tuple_impls {
//     ($(
//         $Tuple:tt {
//             $(($idx:tt) -> $T:ident,)+
//         }
//     )+) => {$(
//         impl<$($T,)+> Handler for ($($T,)+) where $($T: Handler,)+
//         {
//             async fn handle(&self, sconf: Arc<ServerConfig>, req: &Request, depot: &mut Depot, resp: &mut Response) {
//                 $(
//                     if !resp.is_commited() {
//                         self.$idx.handle(sconf.clone(), req, depot, resp).await;
//                     }
//                 )+
//             }
//         })+
//     }
// }
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

// __for_each_handler_tuple!(handler_tuple_impls);