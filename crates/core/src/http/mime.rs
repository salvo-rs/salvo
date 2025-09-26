//! Reexport of the `mime` crate and some mime related utilities.

pub use mime::*;

use crate::http::Request;

#[doc(hidden)]
#[inline]
pub fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or(mime::TEXT_HTML);
    let accept = req.accept();
    accept
        .first()
        .unwrap_or(&dmime)
        .to_string()
        .parse()
        .unwrap_or(dmime)
}

#[doc(hidden)]
#[inline]
#[must_use]
pub fn detect_text_mime(buffer: &[u8]) -> Option<Mime> {
    let info = content_inspector::inspect(buffer);
    if info.is_text() {
        if let Some(charset) = detect_text_charset(buffer) {
            if charset.eq_ignore_ascii_case("utf-8") {
                Some(mime::TEXT_PLAIN_UTF_8)
            } else {
                format!("text/plain; charset={charset}")
                    .parse::<Mime>()
                    .ok()
            }
        } else {
            Some(mime::TEXT_PLAIN_UTF_8)
        }
    } else {
        None
    }
}

#[doc(hidden)]
#[inline]
#[must_use]
pub fn detect_text_charset(buffer: &[u8]) -> Option<String> {
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(buffer, buffer.len() < 1024);

    let (encoding, _) = detector.guess_assess(None, true);
    if encoding.name().eq_ignore_ascii_case("utf-8") {
        Some("utf-8".into())
    } else {
        Some(encoding.name().into())
    }
}

#[doc(hidden)]
#[inline]
#[must_use]
pub fn is_charset_required_mime(mime: &Mime) -> bool {
    matches!(mime.subtype(), mime::JAVASCRIPT | mime::XML | mime::JSON)
        || matches!(mime.type_(), mime::TEXT)
}

#[doc(hidden)]
#[inline]
pub fn fill_mime_charset_if_need(mime: &mut Mime, buffer: &[u8]) {
    if !is_charset_required_mime(mime) || mime.get_param("charset").is_some() {
        return;
    }
    if let Some(charset) = detect_text_charset(buffer) {
        if let Ok(new_mime) = format!("{mime}; charset={charset}").parse::<Mime>() {
            *mime = new_mime;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::header::*;

    #[test]
    fn test_guess_accept_mime() {
        let mut req = Request::default();
        let headers = req.headers_mut();
        headers.insert(ACCEPT, HeaderValue::from_static("application/javascript"));
        let mime = guess_accept_mime(&req, None);
        assert_eq!(mime, "application/javascript".parse::<Mime>().unwrap());
    }
}
