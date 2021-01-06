mod and;
// mod and_then;
pub(crate) mod impls;
mod or;
// mod or_else;

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use futures::{future, TryFuture, TryFutureExt};

pub(crate) use self::and::And;
// use self::and_then::AndThen;
pub(crate) use self::or::Or;
// use self::or_else::OrElse;
use crate::http::Request;
use crate::routing::{PathState, Router};

#[async_trait]
pub trait Filter {
    fn and<F>(self, other: F) -> And<Self, F>
    where
        Self: Sized,
        F: Filter + Clone,
    {
        And { first: self, second: other }
    }

    fn or<F>(self, other: F) -> Or<Self, F>
    where
        Self: Filter + Sized,
        F: Filter,
    {
        Or { first: self, second: other }
    }

    // fn and_then<F>(self, fun: F) -> AndThen<Self, F>
    // where
    //     Self: Sized,
    //     F: Fn() -> Filter,
    // {
    //     AndThen { filter: self, callback: fun }
    // }

    // fn or_else<F>(self, fun: F) -> OrElse<Self, F>
    // where
    //     Self: Filter,
    //     F: Fn() -> Filter,
    // {
    //     OrElse { filter: self, callback: fun }
    // }

    async fn execute(&self, req: &mut Request, path: &mut PathState) -> bool;
}

// ===== FilterFn =====
// pub(crate) fn filter_fn<F, U>(func: F) -> FilterFn<F>
// where
//     F: Fn(&mut Request, &mut PathState) -> bool,
// {
//     FilterFn { func }
// }

// #[derive(Copy, Clone)]
// #[allow(missing_debug_implementations)]
// pub(crate) struct FilterFn<F> {
//     func: F,
// }

// #[async_trait]
// impl<F, U> Filter for FilterFn<F>
// where
//     F: Fn(&mut Request, &mut PathState) -> U,
//     U: Future<Output = bool>,
// {
//     #[inline]
//     async fn execute(&self, req: &mut Request, path: &mut PathState) -> bool {
//         self.func(req, path)
//     }
// }

// pub trait Func<Args> {
//     type Output;

//     fn call(&self, args: Args) -> Self::Output;
// }

// impl<F, R> Func<()> for F
// where
//     F: Fn() -> R,
// {
//     type Output = R;

//     #[inline]
//     fn call(&self, _args: ()) -> Self::Output {
//         (*self)()
//     }
// }
