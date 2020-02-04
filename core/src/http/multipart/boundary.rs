use futures::{Stream, TryStream};

use std::{fmt, mem};

use crate::http::BodyChunk;

use self::State::*;

use super::helpers::*;
use std::task::{Poll, Context};
use std::pin::Pin;
use crate::http::errors::ReadError;

/// A struct implementing `Read` and `BufRead` that will yield bytes until it sees a given sequence.
pub struct BoundaryFinder<S: TryStream> where S::Error: Into<ReadError> {
    stream: S,
    state: State<S::Ok>,
    boundary: Box<[u8]>,
}

impl<S: TryStream> BoundaryFinder<S> where S::Error: Into<ReadError> {
    pub fn new<B: Into<Vec<u8>>>(stream: S, boundary: B) -> Self {
        BoundaryFinder {
            stream,
            state: State::Watching,
            boundary: boundary.into().into_boxed_slice(),
        }
    }
}

macro_rules! set_state {
    ($self:ident = $state:expr) => {
        *$self.as_mut().state() = $state;
    };
}

impl<S> BoundaryFinder<S>
where
    S: TryStream,
    S::Ok: BodyChunk,
    S::Error: Into<ReadError>,
{
    unsafe_pinned!(stream: S);
    unsafe_unpinned!(state: State<S::Ok>);

    pub fn body_chunk(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<S::Ok, ReadError>>> {
        macro_rules! try_ready_opt (
            ($try:expr) => (
                match $try {
                    Poll::Ready(Some(Ok(val))) => val,
                    Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e.into()))),
                    Poll::Ready(None) => {
                        set_state!(self = End);
                        return Poll::Ready(None);
                    }
                    Poll::Pending => return Poll::Pending,
                }
            );
            ($try:expr; $restore:expr) => (
                match $try {
                    Poll::Ready(Some(Ok(val))) => val,
                    Poll::Ready(Some(Err(e))) => {
                        set_state!(self = $restore);
                        return Poll::Ready(Some(Err(e.into())));
                    },
                    Poll::Ready(None) => {
                        set_state!(self = End);
                        return Poll::Ready(None);
                    },
                    Poll::Pending => {
                        set_state!(self = $restore);
                        return Poll::Pending;
                    }
                }
            )
        );

        loop {
            // trace!("body_chunk() loop state: {:?}", self.state,);

            match self.state {
                Found(_) | Split(_, _) | End => return Poll::Ready(None),
                _ => (),
            }

            match mem::replace(self.as_mut().state(), Watching) {
                Watching => {
                    let chunk = try_ready_opt!(self.as_mut().stream().try_poll_next(cx));

                    // For sanity
                    if chunk.is_empty() {
                        return ready_ok(chunk);
                    }

                    if let Some(chunk) = self.as_mut().check_chunk(chunk) {
                        return ready_ok(chunk);
                    }
                }
                Remainder(rem) => {
                    if let Some(chunk) = self.as_mut().check_chunk(rem) {
                        return ready_ok(chunk);
                    }
                }
                Partial(partial, res) => {
                    let chunk = match self.as_mut().stream().try_poll_next(cx) {
                        Poll::Ready(Some(chunk)) => {
                            match chunk {
                                Ok(chunk) => chunk,
                                Err(e) => return Poll::Ready(Some(Err(e.into()))),
                            }
                        },
                        Poll::Ready(None) => {
                            set_state!(self = End);
                            return Poll::Ready(Some(fmt_err!(
                                "unable to verify multipart boundary; expected: \"{}\" found: \"{}\"",
                                show_bytes(&self.boundary),
                                show_bytes(partial.as_slice())
                            )));
                        }
                        Poll::Pending => {
                            set_state!(self = Partial(partial, res));
                            return Poll::Pending;
                        }
                    };

                    // trace!("Partial got second chunk: {}", show_bytes(chunk.as_slice()));

                    if !self.is_boundary_prefix(partial.as_slice(), chunk.as_slice(), res) {
                        // trace!("partial + chunk don't make a boundary prefix");
                        set_state!(self = Remainder(chunk));
                        return ready_ok(partial);
                    }

                    let needed_len =
                        (self.boundary_size(res.incl_crlf)).saturating_sub(partial.len());

                    if needed_len > chunk.len() {
                        // hopefully rare; must be dealing with a poorly behaved stream impl
                        return Poll::Ready(
                            Some(fmt_err!(
                                "needed {} more bytes to verify boundary, got {}",
                                needed_len,
                                chunk.len()
                            )),
                        );
                    }

                    let bnd_start = res.boundary_start();

                    let is_boundary = (bnd_start > partial.len()
                        // `partial` ended with a `<CR>` and `chunk` starts with `<LF>--<boundary>`
                        && self.check_boundary(&chunk.as_slice()[bnd_start - partial.len()..]))
                        || self.check_boundary_split(
                            &partial.as_slice()[bnd_start..],
                            chunk.as_slice(),
                        );

                    if !is_boundary {
                        // trace!("partial + chunk don't make a whole boundary");
                        *self.as_mut().state() = Remainder(chunk);
                        return ready_ok(partial);
                    }

                    let ret = if res.incl_crlf {
                        if partial.len() < bnd_start {
                            // `partial` ended with a `<CR>` and `chunk` starts with `<LF>--<boundary>`
                            *self.as_mut().state() =
                                Found(chunk.split_into(bnd_start - partial.len()).1);
                            partial.split_into(res.idx).0
                        } else {
                            let (ret, rem) = partial.split_into(res.idx);
                            let (_, first) = rem.split_into(2);
                            *self.as_mut().state() = Split(first, chunk);
                            ret
                        }
                    } else {
                        let (ret, first) = partial.split_into(res.idx);
                        *self.as_mut().state() = Split(first, chunk);
                        ret
                    };

                    if !ret.is_empty() {
                        return ready_ok(ret);
                    } else {
                        // Don't return an empty chunk at the end
                        return Poll::Ready(None);
                    }
                }
                state => unreachable!("invalid state: {:?}", state),
            }
        }
    }

    fn check_chunk(mut self: Pin<&mut Self>, chunk: S::Ok) -> Option<S::Ok> {
        // trace!("check chunk: '{}'", show_bytes(chunk.as_slice()));

        if chunk.is_empty() {
            return None;
        }

        if let Some(res) = self.find_boundary(&chunk) {
            // debug!("boundary found: {:?}", res);

            let len = self.boundary_size(res.incl_crlf);

            if chunk.len() < res.idx + len {
                // Either partial boundary, or boundary but not the two bytes after it
                set_state!(self = Partial(chunk, res));
                // trace!("partial boundary: {:?}", self.state);
                None
            } else {
                let (ret, bnd) = chunk.split_into(res.idx);

                let bnd = if res.incl_crlf {
                    // cut off the preceding CRLF
                    bnd.split_into(2).1
                } else {
                    bnd
                };

                set_state!(self = Found(bnd));

                // trace!(
                //     "boundary located: {:?} returning chunk: {}",
                //     self.state,
                //     show_bytes(ret.as_slice())
                // );

                if !ret.is_empty() {
                    Some(ret)
                } else {
                    None
                }
            }
        } else {
            Some(chunk)
        }
    }

    fn find_boundary(&self, chunk: &S::Ok) -> Option<SearchResult> {
        twoway::find_bytes(chunk.as_slice(), &self.boundary)
            .map(|idx| check_crlf(chunk.as_slice(), idx))
            .or_else(|| self.partial_find_boundary(chunk))
    }

    fn is_boundary_prefix(&self, first: &[u8], second: &[u8], res: SearchResult) -> bool {
        let maybe_prefix = first.iter().chain(second);

        if res.incl_crlf {
            maybe_prefix
                .zip(b"\r\n".iter().chain(&*self.boundary))
                .all(|(l, r)| l == r)
        } else {
            maybe_prefix.zip(&*self.boundary).all(|(l, r)| l == r)
        }
    }

    fn partial_find_boundary(&self, chunk: &S::Ok) -> Option<SearchResult> {
        let chunk = chunk.as_slice();
        let len = chunk.len();

        partial_rmatch(chunk, &self.boundary)
            .map(|idx| check_crlf(chunk, idx))
            .or_else(||
                // EDGE CASE: the bytes of the newline before the boundary are at the end
                // of the chunk
                if len >= 2 && &chunk[len - 2..] == &*b"\r\n" {
                    Some(SearchResult {
                        idx: len - 2,
                        incl_crlf: true,
                    })
                } else if len >= 1 && chunk[len - 1] == b'\r' {
                    Some(SearchResult {
                        idx: len - 1,
                        incl_crlf: true,
                    })
                } else {
                    None
                }
            )
    }

    fn check_boundary(&self, bytes: &[u8]) -> bool {
        (bytes.len() >= 2 && bytes[2..].starts_with(&self.boundary))
            || bytes.starts_with(&self.boundary)
    }

    fn check_boundary_split(&self, first: &[u8], second: &[u8]) -> bool {
        let check_len = self.boundary.len().saturating_sub(first.len());

        second.len() >= check_len
            && first
                .iter()
                .chain(&second[..check_len])
                .zip(self.boundary.iter())
                .all(|(l, r)| l == r)
    }

    /// Returns `true` if another field should follow this boundary, `false` if the stream
    /// is at a logical end
    pub fn consume_boundary(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<bool, ReadError>> {
        // debug!("consuming boundary");

        while ready!(self.as_mut().body_chunk(cx)?).is_some() {
            // trace!("body chunk loop!");
        }

        // trace!("consume_boundary() after-loop state: {:?}", self.state,);

        match mem::replace(self.as_mut().state(), Watching) {
            Found(bnd) => self.confirm_boundary(bnd),
            Split(first, second) => self.confirm_boundary_split(first, second),
            End => {
                *self.state() = End;
                ready_ok(false)
            }
            state => unreachable!("invalid state: {:?}", state),
        }
    }

    fn confirm_boundary(
        mut self: Pin<&mut Self>,
        boundary: S::Ok,
    ) -> Poll<Result<bool, ReadError>> {
        if boundary.len() < self.boundary_size(false) {
            ret_err!(
                "boundary sequence too short: {}",
                show_bytes(boundary.as_slice())
            );
        }

        let (boundary, rem) = boundary.split_into(self.boundary_size(false));
        let boundary = boundary.as_slice();

        // trace!("confirming boundary: {}", show_bytes(boundary));

        debug_assert!(
            !boundary.starts_with(b"\r\n"),
            "leading CRLF should have been trimmed from boundary: {}",
            show_bytes(boundary)
        );

        debug_assert!(
            self.check_boundary(boundary),
            "invalid boundary previous confirmed as valid: {}",
            show_bytes(boundary)
        );

        set_state!(
            self = if !rem.is_empty() {
                Remainder(rem)
            } else {
                Watching
            }
        );

        // trace!("boundary found: {}", show_bytes(boundary));

        let is_end = check_last_two(boundary);

        // debug!("is_end: {:?}", is_end);

        if is_end {
            set_state!(self = End);
        }

        ready_ok(!is_end)
    }

    fn confirm_boundary_split(
        mut self: Pin<&mut Self>,
        first: S::Ok,
        second: S::Ok,
    ) -> Poll<Result<bool, ReadError>> {
        let first = first.as_slice();
        let check_len = self.boundary_size(false) - first.len();

        if second.len() < check_len {
            ret_err!(
                "split boundary sequence too short: ({}, {})",
                show_bytes(first),
                show_bytes(second.as_slice())
            );
        }

        let (second, rem) = second.split_into(check_len);
        let second = second.as_slice();

        set_state!(self = Remainder(rem));

        debug_assert!(
            !first.starts_with(b"\r\n"),
            "leading CRLF should have been trimmed from first boundary section: {}",
            show_bytes(first)
        );

        debug_assert!(
            self.check_boundary_split(first, second),
            "invalid split boundary previous confirmed as valid: ({}, {})",
            show_bytes(first),
            show_bytes(second)
        );

        let is_end = check_last_two(second);

        if is_end {
            set_state!(self = End);
        }

        ready_ok(!is_end)
    }

    /// The necessary size to verify a boundary, including the potential CRLF before, and the
    /// CRLF / "--" afterward
    fn boundary_size(&self, incl_crlf: bool) -> usize {
        self.boundary.len() + if incl_crlf { 4 } else { 2 }
    }
}

impl<S> Stream for BoundaryFinder<S>
where
    S: TryStream,
    S::Ok: BodyChunk,
    S::Error: Into<ReadError>,
{
    type Item = Result<S::Ok, ReadError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.body_chunk(cx)
    }
}

impl<S: TryStream + fmt::Debug> fmt::Debug for BoundaryFinder<S>
where
    S::Ok: BodyChunk + fmt::Debug, 
    S::Error: Into<ReadError>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BoundaryFinder")
            .field("stream", &self.stream)
            .field("state", &self.state)
            .field("boundary", &self.boundary)
            .finish()
    }
}

enum State<B> {
    /// Watching for next boundary
    Watching,
    /// Partial boundary
    Partial(B, SearchResult),
    Found(B),
    Split(B, B),
    /// The remains of a chunk after processing
    Remainder(B),
    End,
}

impl<B: BodyChunk> fmt::Debug for State<B> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::State::*;

        match *self {
            Watching => f.write_str("State::Watching"),
            Partial(ref bnd, res) => write!(
                f,
                "State::Partial({}, {:?})",
                show_bytes(bnd.as_slice()),
                res
            ),
            Found(ref bnd) => write!(f, "State::Found({})", show_bytes(bnd.as_slice())),
            Split(ref first, ref second) => write!(
                f,
                "State::Split(\"{}\", \"{}\")",
                show_bytes(first.as_slice()),
                show_bytes(second.as_slice())
            ),
            Remainder(ref rem) => write!(f, "State::Remainder({})", show_bytes(rem.as_slice())),
            End => f.write_str("State::End"),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct SearchResult {
    idx: usize,
    incl_crlf: bool,
}

impl SearchResult {
    fn boundary_start(&self) -> usize {
        if self.incl_crlf {
            self.idx + 2
        } else {
            self.idx
        }
    }
}

/// If there's a CRLF before the boundary, we want to back up to make sure we don't yield a newline
/// that the client doesn't expect
fn check_crlf(chunk: &[u8], mut idx: usize) -> SearchResult {
    let mut incl_crlf = false;
    if idx >= 2 && chunk[idx - 2..idx] == *b"\r\n" {
        incl_crlf = true;
        idx -= 2;
    }

    SearchResult { idx, incl_crlf }
}

fn check_last_two(boundary: &[u8]) -> bool {
    let len = boundary.len();

    let is_end = boundary.ends_with(b"--");

    // if !is_end && !boundary.ends_with(b"\r\n") && boundary.len() > 2 {
        // warn!(
        //     "unexpected bytes after boundary: {:?} ('--': {:?}, '\\r\\n': {:?})",
        //     &boundary[len - 2..],
        //     b"--",
        //     b"\r\n"
        // );
    // }

    is_end
}

/// Check if `needle` is cut off at the end of `haystack`, and if so, its index
fn partial_rmatch(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if haystack.is_empty() || needle.is_empty() {
        return None;
    }

    // If the haystack is smaller than the needle, we still need to test it
    let trim_start = haystack.len().saturating_sub(needle.len() - 1);

    let idx = try_opt!(twoway::find_bytes(&haystack[trim_start..], &needle[..1])) + trim_start;

    // trace!("partial_rmatch found start: {:?}", idx);

    // If the rest of `haystack` matches `needle`, then we have our partial match
    if haystack[idx..].iter().zip(needle).all(|(l, r)| l == r) {
        Some(idx)
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use super::BoundaryFinder;

    use crate::http::errors::ReadError;

    use crate::http::multipart::test_util::*;

    #[test]
    fn test_empty_stream() {
        let finder = BoundaryFinder::new(mock_stream(&[]), BOUNDARY);
        pin_mut!(finder);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(false));
    }

    #[test]
    fn test_one_boundary() {
        let finder = BoundaryFinder::new(mock_stream(&[b"--boundary\r\n"]), BOUNDARY);
        pin_mut!(finder);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(true));
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(false));
    }

    #[test]
    fn test_one_incomplete_boundary() {
        let finder = BoundaryFinder::new(mock_stream(&[b"--bound"]), BOUNDARY);
        pin_mut!(finder);
        ready_assert_eq!(
            |cx| finder.as_mut().consume_boundary(cx),
            Err(ReadError::Parsing(
                "unable to verify multipart boundary; expected: \"--boundary\" found: \"--bound\""
                    .into()
            ))
        );
    }

    #[test]
    fn test_one_empty_field() {
        let finder = BoundaryFinder::new(
            mock_stream(&[b"--boundary", b"\r\n", b"\r\n", b"--boundary--"]),
            BOUNDARY,
        );
        pin_mut!(finder);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(true));
        ready_assert_eq!(|cx| finder.as_mut().body_chunk(cx), None);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(false));
    }

    #[test]
    fn test_one_nonempty_field() {
        let finder = BoundaryFinder::new(
            mock_stream(&[
                b"--boundary",
                b"\r\n",
                b"field data",
                b"\r\n",
                b"--boundary--",
            ]),
            BOUNDARY,
        );
        pin_mut!(finder);

        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(true));
        ready_assert_eq!(
            |cx| finder.as_mut().body_chunk(cx),
            Some(Ok(&b"field data"[..]))
        );
        ready_assert_eq!(|cx| finder.as_mut().body_chunk(cx), None);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(false));
    }

    #[test]
    fn test_two_empty_fields() {
        let finder = BoundaryFinder::new(
            mock_stream(&[
                b"--boundary",
                b"\r\n",
                b"\r\n--boundary\r\n",
                b"\r\n",
                b"--boundary--",
            ]),
            BOUNDARY,
        );
        pin_mut!(finder);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(true));
        ready_assert_eq!(|cx| finder.as_mut().body_chunk(cx), None);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(true));
        ready_assert_eq!(|cx| finder.as_mut().body_chunk(cx), None);
        ready_assert_eq!(|cx| finder.as_mut().consume_boundary(cx), Ok(false));
    }
}
