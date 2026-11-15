//! Reexport of the `mime` crate and some mime related utilities.

pub use mime::*;

use crate::http::Request;

#[doc(hidden)]
#[inline]
pub fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    accepted_ranges(req)
        .into_iter()
        .enumerate()
        .filter(|(_, (range, quality))| {
            *quality > 0 && range.type_() != STAR && range.subtype() != STAR
        })
        .max_by_key(|(order, (_, quality))| (*quality, std::cmp::Reverse(*order)))
        .map(|(_, (mime, _))| without_quality(&mime))
        .unwrap_or_else(|| default_type.unwrap_or(mime::TEXT_HTML))
}

// Prefer the quality of the most specific matching range, rather than the
// highest quality wildcard. An explicit q=0 therefore excludes that format.
pub(crate) fn negotiate_accept_mime(req: &Request, supported: &[Mime]) -> Option<Mime> {
    let ranges = accepted_ranges(req);
    supported
        .iter()
        .enumerate()
        .filter_map(|(order, candidate)| {
            let best =
                ranges
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (range, quality))| {
                        if range.type_() != STAR && range.type_() != candidate.type_()
                            || range.subtype() != STAR && range.subtype() != candidate.subtype()
                        {
                            return None;
                        }
                        let parameter_count =
                            range.params().take_while(|(name, _)| *name != "q").count();
                        if !range.params().take_while(|(name, _)| *name != "q").all(
                            |(name, value)| {
                                candidate
                                    .get_param(name)
                                    .is_some_and(|actual| actual == value)
                            },
                        ) {
                            return None;
                        }
                        let specificity = (
                            u8::from(range.type_() != STAR),
                            u8::from(range.subtype() != STAR),
                            parameter_count,
                        );
                        Some((specificity, std::cmp::Reverse(index), *quality))
                    })
                    .max_by_key(|(specificity, order, _)| (*specificity, *order));
            best.filter(|(_, _, quality)| *quality > 0)
                .map(|(_, _, quality)| (quality, std::cmp::Reverse(order), candidate))
        })
        .max_by_key(|(quality, order, _)| (*quality, *order))
        .map(|(_, _, mime)| mime.clone())
}

fn without_quality(mime: &Mime) -> Mime {
    let mut value = mime.essence_str().to_owned();
    for (name, parameter) in mime.params().take_while(|(name, _)| *name != "q") {
        value.push_str(&format!("; {name}={parameter}"));
    }
    value
        .parse()
        .unwrap_or_else(|_| mime.essence_str().parse().expect("MIME essence is valid"))
}

fn accepted_ranges(req: &Request) -> Vec<(Mime, u16)> {
    let mut ranges = Vec::new();
    for field in req.headers().get_all(crate::http::header::ACCEPT) {
        let Ok(field) = field.to_str() else { continue };
        let mut quoted = false;
        let mut escaped = false;
        let mut start = 0;
        for (index, byte) in field.bytes().chain(std::iter::once(b',')).enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            match byte {
                b'\\' if quoted => escaped = true,
                b'"' => quoted = !quoted,
                b',' if !quoted => {
                    if let Ok(mime) = field[start..index].trim().parse::<Mime>() {
                        let quality = mime
                            .get_param("q")
                            .map(|q| quality(q.as_str()))
                            .unwrap_or(1000);
                        ranges.push((mime, quality));
                    }
                    start = index + 1;
                }
                _ => {}
            }
        }
    }
    ranges
}

fn quality(value: &str) -> u16 {
    let value = value.trim_matches('"');
    let (integer, decimals) = value.split_once('.').unwrap_or((value, ""));
    if decimals.len() > 3 || !decimals.bytes().all(|digit| digit.is_ascii_digit()) {
        return 0;
    }
    match integer {
        "1" if decimals.bytes().all(|digit| digit == b'0') => 1000,
        "0" => decimals.parse::<u16>().unwrap_or(0) * 10u16.pow((3 - decimals.len()) as u32),
        _ => 0,
    }
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
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(buffer, buffer.len() < 1024);

    let encoding = detector.guess(None, chardetng::Utf8Detection::Allow);
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
    if let Some(charset) = detect_text_charset(buffer)
        && let Ok(new_mime) = format!("{mime}; charset={charset}").parse::<Mime>()
    {
        *mime = new_mime;
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
