pub fn split_even<'a>(data: &'a [u8], parts: usize) -> Vec<&'a [u8]> {
    assert!(parts > 0);
    if data.is_empty() {
        return vec![&[]; parts];
    }

    (0..parts)
        .map(|index| {
            let start = index * data.len() / parts;
            let end = (index + 1) * data.len() / parts;
            &data[start..end]
        })
        .collect()
}

pub fn map_to_charset(bytes: &[u8], charset: &[u8], max_len: usize) -> String {
    if bytes.is_empty() || charset.is_empty() || max_len == 0 {
        return String::new();
    }

    bytes
        .iter()
        .take(max_len)
        .map(|byte| charset[*byte as usize % charset.len()] as char)
        .collect()
}

pub fn safe_token(bytes: &[u8], max_len: usize) -> String {
    map_to_charset(
        bytes,
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-",
        max_len,
    )
}

pub fn safe_header_token(bytes: &[u8], max_len: usize) -> String {
    map_to_charset(
        bytes,
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&'*+-.^_`|~",
        max_len,
    )
}

pub fn safe_host(bytes: &[u8], max_len: usize) -> String {
    map_to_charset(
        bytes,
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-:",
        max_len,
    )
}

pub fn safe_uri_path(bytes: &[u8], max_len: usize) -> String {
    let mapped = map_to_charset(
        bytes,
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~/",
        max_len,
    );
    let trimmed = mapped.trim_matches('/');
    if trimmed.is_empty() {
        "/".to_owned()
    } else {
        format!("/{trimmed}")
    }
}

pub fn safe_route_path(bytes: &[u8], max_len: usize) -> String {
    safe_uri_path(bytes, max_len)
}

pub fn non_empty_token(bytes: &[u8], fallback: &str, max_len: usize) -> String {
    let token = safe_token(bytes, max_len);
    if token.is_empty() {
        fallback.to_owned()
    } else {
        token
    }
}

pub fn token_list(bytes: &[u8], max_tokens: usize, max_token_len: usize) -> Vec<String> {
    split_even(bytes, max_tokens)
        .into_iter()
        .map(|segment| safe_token(segment, max_token_len))
        .filter(|token| !token.is_empty())
        .collect()
}
