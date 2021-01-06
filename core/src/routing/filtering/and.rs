use std::future::{ready, Future};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use pin_project::pin_project;

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct And<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for And<T, U>
where
    T: Filter,
    U: Filter + Clone + Send,
{
    type Future = AndFuture<T, U>;
    #[inline]
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        async move {
            if !self.first.execute(req, path).await {
                false
            } else {
                self.second.execute(req, path).await
            }
        }
    }
}

#[allow(missing_debug_implementations)]
#[pin_project]
pub struct AndFuture<T: Filter, U: Filter> {
    #[pin]
    state: State<T::Future, T::Extract, U>,
}

#[pin_project(project = StateProj)]
enum State<T, TE, U: Filter> {
    First(#[pin] T, U),
    Second(Option<TE>, #[pin] U::Future),
    Done,
}

impl<T, U> Future for AndFuture<T, U>
where
    T: Filter,
    U: Filter,
    <T::Extract as Tuple>::HList: Combine<<U::Extract as Tuple>::HList> + Send,
    U::Error: CombineRejection<T::Error>,
{
    type Output = Result<CombinedTuples<T::Extract, U::Extract>, <U::Error as CombineRejection<T::Error>>::One>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().state.poll(cx)
    }
}

impl<T, TE, U, E> Future for State<T, TE, U>
where
    T: Future<Output = Result<TE, E>>,
    U: Filter,
    TE: Tuple,
    TE::HList: Combine<<U::Extract as Tuple>::HList> + Send,
    U::Error: CombineRejection<E>,
{
    type Output = Result<CombinedTuples<TE, U::Extract>, <U::Error as CombineRejection<E>>::One>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.as_mut().project() {
                StateProj::First(first, second) => {
                    let ex1 = ready!(first.poll(cx))?;
                    let fut2 = second.filter(Internal);
                    self.set(State::Second(Some(ex1), fut2));
                }
                StateProj::Second(ex1, second) => {
                    let ex2 = ready!(second.poll(cx))?;
                    let ex3 = ex1.take().unwrap().combine(ex2);
                    self.set(State::Done);
                    return Poll::Ready(Ok(ex3));
                }
                StateProj::Done => panic!("polled after complete"),
            }
        }
    }
}
