use async_trait::async_trait;

use crate::http::{Request, Response};
use crate::Depot;

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    #[must_use = "handle future must be used"]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response);
}

macro_rules! handler_tuple_impls {
    ($(
        $Tuple:tt {
            $(($idx:tt) -> $T:tt,)+ //https://github.com/dtolnay/async-trait/issues/46
        }
    )+) => {$(
        #[async_trait]
        impl<$($T,)+> Handler for ($($T,)+) where $($T: Handler,)+
        {
            async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
                $(
                    if !res.is_committed() {
                        self.$idx.handle(req, depot, res).await;
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
