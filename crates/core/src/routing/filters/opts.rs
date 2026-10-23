use std::fmt::{self, Formatter};

use crate::async_trait;
use crate::http::{Method, Request};
use crate::routing::{Filter, PathState};

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

#[async_trait]
impl<T, U> Filter for Or<T, U>
where
    T: Filter + Send,
    U: Filter + Send,
{
    fn collect_methods(&self, methods: &mut Vec<Method>) {
        self.first.collect_methods(methods);
        self.second.collect_methods(methods);
    }

    async fn filter_for_method(
        &self,
        req: &mut Request,
        state: &mut PathState<'_>,
        method: &Method,
    ) -> Option<bool> {
        let checkpoint = state.checkpoint();
        let first = self.first.filter_for_method(req, state, method).await;
        if first == Some(true) {
            return first;
        }
        state.rollback(checkpoint);
        first?;
        let second = self.second.filter_for_method(req, state, method).await;
        if second != Some(true) {
            state.rollback(checkpoint);
        }
        second
    }

    #[inline]
    async fn filter(&self, req: &mut Request, state: &mut PathState<'_>) -> bool {
        let checkpoint = state.checkpoint();
        if self.first.filter(req, state).await {
            true
        } else {
            state.rollback(checkpoint);
            if self.second.filter(req, state).await {
                true
            } else {
                state.rollback(checkpoint);
                false
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct OrElse<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}
#[async_trait]
impl<T, F> Filter for OrElse<T, F>
where
    T: Filter,
    F: for<'a> Fn(&mut Request, &mut PathState<'a>) -> bool + Send + Sync + 'static,
{
    #[inline]
    async fn filter(&self, req: &mut Request, state: &mut PathState<'_>) -> bool {
        let checkpoint = state.checkpoint();
        if self.filter.filter(req, state).await {
            true
        } else {
            state.rollback(checkpoint);
            if (self.callback)(req, state) {
                true
            } else {
                state.rollback(checkpoint);
                false
            }
        }
    }
}

impl<T, F> fmt::Debug for OrElse<T, F> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "opt:or_else")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct And<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

#[async_trait]
impl<T, U> Filter for And<T, U>
where
    T: Filter,
    U: Filter,
{
    fn collect_methods(&self, methods: &mut Vec<Method>) {
        self.first.collect_methods(methods);
        self.second.collect_methods(methods);
    }

    async fn filter_for_method(
        &self,
        req: &mut Request,
        state: &mut PathState<'_>,
        method: &Method,
    ) -> Option<bool> {
        let checkpoint = state.checkpoint();
        let matched = match self.first.filter_for_method(req, state, method).await {
            Some(true) => self.second.filter_for_method(req, state, method).await,
            other => other,
        };
        if matched != Some(true) {
            state.rollback(checkpoint);
        }
        matched
    }

    #[inline]
    async fn filter(&self, req: &mut Request, state: &mut PathState<'_>) -> bool {
        let checkpoint = state.checkpoint();
        if self.first.filter(req, state).await && self.second.filter(req, state).await {
            true
        } else {
            state.rollback(checkpoint);
            false
        }
    }
}

#[derive(Clone, Copy)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

#[async_trait]
impl<T, F> Filter for AndThen<T, F>
where
    T: Filter,
    F: for<'a> Fn(&mut Request, &mut PathState<'a>) -> bool + Send + Sync + 'static,
{
    #[inline]
    async fn filter(&self, req: &mut Request, state: &mut PathState<'_>) -> bool {
        let checkpoint = state.checkpoint();
        if self.filter.filter(req, state).await && (self.callback)(req, state) {
            true
        } else {
            state.rollback(checkpoint);
            false
        }
    }
}

impl<T, F> fmt::Debug for AndThen<T, F> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "opt:and_then")
    }
}

#[cfg(test)]
mod tests {
    use crate::routing::{Filter, PathState, filters};
    use crate::test::TestClient;

    #[tokio::test]
    async fn or_retries_from_the_original_path() {
        let filter = filters::path("a")
            .and(filters::get())
            .or(filters::path("a").and(filters::post()));
        let mut req = TestClient::post("http://localhost/a").build();
        let mut state = PathState::from_borrowed_path("/a");
        assert!(filter.filter(&mut req, &mut state).await);
        assert!(state.is_ended());
    }

    #[tokio::test]
    async fn or_discards_failed_captures() {
        let filter = filters::path("{leaked}/wrong").or(filters::path("{actual}/right"));
        let mut req = TestClient::get("http://localhost/alice/right").build();
        let mut state = PathState::from_borrowed_path("/alice/right");
        assert!(filter.filter(&mut req, &mut state).await);
        assert!(!state.params.contains_key("leaked"));
        assert_eq!(state.params["actual"], "alice");
        #[cfg(feature = "matched-path")]
        assert_eq!(state.matched_parts, ["{actual}", "right"]);
    }

    #[tokio::test]
    async fn or_rolls_back_failed_wildcard_captures() {
        let filter = filters::path("{**ignored}")
            .and(filters::post())
            .or(filters::path("{**actual}").and(filters::get()));
        let mut req = TestClient::get("http://localhost/some/path").build();
        let mut state = PathState::from_borrowed_path("/some/path");
        assert!(filter.filter(&mut req, &mut state).await);
        assert!(!state.params.contains_key("ignored"));
        assert_eq!(state.params.tail(), Some("some/path"));
        assert_eq!(state.params["actual"], "some/path");
    }

    #[tokio::test]
    async fn failed_composites_restore_ancestor_captures() {
        let mut req = TestClient::get("http://localhost/alice/edit").build();
        let mut state = PathState::from_borrowed_path("/alice/edit");
        assert!(filters::path("{id}").filter(&mut req, &mut state).await);
        let before = state.clone();
        let filter = filters::path("{id}").and(filters::post());
        assert!(!filter.filter(&mut req, &mut state).await);
        assert_eq!(state, before);

        let filter = filters::path("{id}").and_then(|_, _| false);
        assert!(!filter.filter(&mut req, &mut state).await);
        assert_eq!(state, before);

        let filter = filters::path("{id}/missing")
            .or_else(|_, state| state.pick() == Some("edit") && state.params["id"] == "alice");
        assert!(filter.filter(&mut req, &mut state).await);
        assert_eq!(state, before);
    }

    #[tokio::test]
    async fn failed_alternatives_restore_the_input_state() {
        let mut req = TestClient::get("http://localhost/alice").build();
        let mut state = PathState::from_borrowed_path("/alice");
        let before = state.clone();
        let filter = filters::path("{first}")
            .and(filters::post())
            .or(filters::path("{second}").and(filters::delete()));
        assert!(!filter.filter(&mut req, &mut state).await);
        assert_eq!(state, before);

        let filter = filters::path("{id}/missing").or_else(|_, state| {
            state.forward(1);
            false
        });
        assert!(!filter.filter(&mut req, &mut state).await);
        assert_eq!(state, before);

        assert!(!filters::path("{id}/missing").detect(&mut state));
        assert_eq!(state, before);
    }
}
