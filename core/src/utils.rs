pub fn decode_url_path(path: &str) -> String {
    format!("/{}", decode_url_path_segments(path).join("/"))
}

pub fn decode_url_path_segments(path: &str) -> Vec<String> {
    let segments = path.trim_start_matches('/').split('/');
    segments
        .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
        .filter(|s| !s.contains('/') && !s.is_empty())
        .collect::<Vec<_>>()
}