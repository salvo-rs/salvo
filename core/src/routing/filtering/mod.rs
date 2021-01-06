mod and;
mod and_then;
pub(crate) mod impls;
mod or;
mod or_else;

use std::future::Future;
use std::pin::Pin;

use futures::{future, TryFuture, TryFutureExt};

pub(crate) use self::and::And;
use self::and_then::AndThen;
pub(crate) use self::or::Or;
use self::or_else::OrElse;
use crate::http::Request;
use crate::routing::{PathState, Router};

/// Composable request filters.
///
/// A `Filter` can optionally extract some data from a request, combine
/// it with others, mutate it, and return back some value as a reply. The
/// power of `Filter`s come from being able to isolate small subsets, and then
/// chain and reuse them in various parts of your app.
///
/// # Extracting Tuples
///
/// You may notice that several of these filters extract some tuple, often
/// times a tuple of just 1 item! Why?
///
/// If a filter extracts a `(String,)`, that simply means that it
/// extracts a `String`. If you were to `map` the filter, the argument type
/// would be exactly that, just a `String`.
///
/// What is it? It's just some type magic that allows for automatic combining
/// and flattening of tuples. Without it, combining two filters together with
/// `and`, where one extracted `()`, and another `String`, would mean the
/// `map` would be given a single argument of `((), String,)`, which is just
/// no fun.
pub trait Filter {
    type Future: Future<Output = bool>;

    /// Composes a new `Filter` that requires both this and the other to filter a request.
    ///
    /// Additionally, this will join together the extracted values of both
    /// filters, so that `map` and `and_then` receive them as separate arguments.
    ///
    /// If a `Filter` extracts nothing (so, `()`), combining with any other
    /// filter will simply discard the `()`. If a `Filter` extracts one or
    /// more items, combining will mean it extracts the values of itself
    /// combined with the other.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo::Filter;
    ///
    /// // Match `/hello/:name`...
    /// salvo::path("hello")
    ///     .and(salvo::path::param::<String>());
    /// ```
    fn and<F>(self, other: F) -> And<Self, F>
    where
        Self: Sized,
        F: Filter + Clone,
    {
        And { first: self, second: other }
    }

    /// Composes a new `Filter` of either this or the other filter.
    ///
    /// # Example
    ///
    /// ```
    /// use std::net::SocketAddr;
    /// use salvo::Filter;
    ///
    /// // Match either `/:u32` or `/:socketaddr`
    /// salvo::path::param::<u32>()
    ///     .or(salvo::path::param::<SocketAddr>());
    /// ```
    fn or<F>(self, other: F) -> Or<Self, F>
    where
        Self: Filter + Sized,
        F: Filter,
    {
        Or { first: self, second: other }
    }

    /// Composes this `Filter` with a function receiving the extracted value.
    ///
    /// The function should return some `TryFuture` type.
    ///
    /// The `Error` type of the return `Future` needs be a `Rejection`, which
    /// means most futures will need to have their error mapped into one.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo::Filter;
    ///
    /// // Validate after `/:id`
    /// salvo::path::param().and_then(|id: u64| async move {
    ///     if id != 0 {
    ///         Ok(format!("Hello #{}", id))
    ///     } else {
    ///         Err(salvo::reject::not_found())
    ///     }
    /// });
    /// ```
    fn and_then<F>(self, fun: F) -> AndThen<Self, F>
    where
        Self: Sized,
        F: Fn() -> Filter<Future = Future<Output = bool>> + Clone,
    {
        AndThen { filter: self, callback: fun }
    }

    /// Compose this `Filter` with a function receiving an error.
    ///
    /// The function should return some `TryFuture` type yielding the
    /// same item and error types.
    fn or_else<F>(self, fun: F) -> OrElse<Self, F>
    where
        Self: Filter<Future = Future<Output = bool>> + Sized,
        F: Fn() -> Filter<Future = Future<Output = bool>> + Send,
    {
        OrElse { filter: self, callback: fun }
    }

    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future;
}

// ===== FilterFn =====
pub(crate) fn filter_fn<F, U>(func: F) -> FilterFn<F>
where
    F: Fn(&mut Request, &mut PathState) -> Future<Output = bool>,
{
    FilterFn { func }
}

#[derive(Copy, Clone)]
#[allow(missing_debug_implementations)]
pub(crate) struct FilterFn<F> {
    func: F,
}

impl<F, U> Filter for FilterFn<F>
where
    F: Fn(&mut Request, &mut PathState) -> U,
    U: Future<Output = bool>,
{
    type Future = Future<Output = bool>;
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        self.func(req, path).into_future()
    }
}

pub trait Func<Args> {
    type Output;

    fn call(&self, args: Args) -> Self::Output;
}

impl<F, R> Func<()> for F
where
    F: Fn() -> R,
{
    type Output = R;

    #[inline]
    fn call(&self, _args: ()) -> Self::Output {
        (*self)()
    }
}
