use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{ready, TryFuture};
use pin_project::pin_project;

use crate::http::Request;
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> Filter for Or<T, U>
where
    T: Filter,
    U: Filter + Send,
{
    type Future = EitherFuture<T, U>;
    fn execute(&self, req: &mut Request, path: &mut PathState) -> Self::Future {
        EitherFuture {
            request: req,
            path,
            state: State::First(self.first.execute(req, path), self.second.clone()),
        }
    }
}

#[allow(missing_debug_implementations)]
#[pin_project]
pub struct EitherFuture<'a, T: Filter, U: Filter> {
    request: &'a Request,
    path: &'a PathState,
    #[pin]
    state: State<T, U>,
}

#[pin_project(project = StateProj)]
enum State<T: Filter, U: Filter> {
    First(#[pin] T::Future, U),
    Second(#[pin] U::Future),
    Done,
}

impl<T, U> Future for EitherFuture<T, U>
where
    T: Filter,
    U: Filter,
{
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            let pin = self.as_mut().project();
            let (err1, fut2) = match pin.state.project() {
                StateProj::First(first, second) => {
                    if ready!(first.poll(cx)) {
                        return Poll::Ready(true);
                    } else {
                        second.execute(self.request, self.path)
                    }
                }
                StateProj::Second(err1, second) => {
                    let ex2 = match ready!(second.try_poll(cx)) {
                        Ok(ex2) => Ok((Either::B(ex2),)),
                        Err(e) => {
                            pin.original_path_index.reset_path();
                            let err1 = err1.take().expect("polled after complete");
                            Err(e.combine(err1))
                        }
                    };
                    self.set(EitherFuture { state: State::Done, ..*self });
                    return Poll::Ready(ex2);
                }
                StateProj::Done => panic!("polled after complete"),
            };

            self.set(EitherFuture {
                state: State::Second(Some(err1), fut2),
                ..*self
            });
        }
    }
}
