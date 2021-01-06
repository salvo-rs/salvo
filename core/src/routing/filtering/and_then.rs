use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{ready, TryFuture};
use pin_project::pin_project;

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> Filter for AndThen<T, F>
where
    T: Filter,
    F: Fn() -> bool + Clone + Send,
{
    type Future = AndThenFuture<T, F>;

    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        async move {
            if !self.filter.execute(req, path).await {
                false
            } else {
                self.callback.call().execute(req, path).await
            }
        }
    }
}

#[allow(missing_debug_implementations)]
#[pin_project]
pub struct AndThenFuture<T: Filter, F>
where
    T: Filter,
    F: Func<T::Extract>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    #[pin]
    state: State<T::Future, F>,
}

#[pin_project(project = StateProj)]
enum State<T, F>
where
    T: TryFuture,
    F: Func<T::Ok>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    First(#[pin] T, F),
    Second(#[pin] F::Output),
    Done,
}

impl<T, F> Future for AndThenFuture<T, F>
where
    T: Filter,
    F: Func<T::Extract>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    type Output = Result<(<F::Output as TryFuture>::Ok,), <<F::Output as TryFuture>::Error as CombineRejection<T::Error>>::One>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().state.poll(cx)
    }
}

impl<T, F> Future for State<T, F>
where
    T: TryFuture,
    F: Func<T::Ok>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    type Output = Result<(<F::Output as TryFuture>::Ok,), <<F::Output as TryFuture>::Error as CombineRejection<T::Error>>::One>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.as_mut().project() {
                StateProj::First(first, second) => {
                    let ex1 = ready!(first.try_poll(cx))?;
                    let fut2 = second.call(ex1);
                    self.set(State::Second(fut2));
                }
                StateProj::Second(second) => {
                    let ex3 = match ready!(second.try_poll(cx)) {
                        Ok(item) => Ok((item,)),
                        Err(err) => Err(From::from(err)),
                    };
                    self.set(State::Done);
                    return Poll::Ready(ex3);
                }
                StateProj::Done => panic!("polled after complete"),
            }
        }
    }
}
