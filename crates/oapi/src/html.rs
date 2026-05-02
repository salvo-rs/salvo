pub(crate) fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub(crate) fn json_string(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".to_owned())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

pub(crate) fn style_text(value: &str) -> String {
    value.replace("</", "<\\/")
}

pub(crate) fn keywords_meta(value: &str) -> String {
    let keywords = value
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(",");
    meta_tag("keywords", &keywords)
}

pub(crate) fn description_meta(value: &str) -> String {
    meta_tag("description", value)
}

fn meta_tag(name: &str, value: &str) -> String {
    format!(
        "<meta name=\"{}\" content=\"{}\">",
        escape_html(name),
        escape_html(value)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(
            escape_html(r#"<script a="b">'&</script>"#),
            "&lt;script a=&quot;b&quot;&gt;&#x27;&amp;&lt;/script&gt;"
        );
    }

    #[test]
    fn test_json_string() {
        assert_eq!(json_string(r#"""#), r#""\"""#);
        assert_eq!(
            json_string("</script>&\u{2028}\u{2029}"),
            r#""\u003c/script\u003e\u0026\u2028\u2029""#
        );
    }

    #[test]
    fn test_style_text_breaks_end_tags() {
        assert_eq!(
            style_text("body{color:red}</style><script>alert(1)</script>"),
            "body{color:red}<\\/style><script>alert(1)<\\/script>"
        );
    }

    #[test]
    fn test_keywords_meta_escapes_content() {
        assert_eq!(
            keywords_meta("a,<b>"),
            r#"<meta name="keywords" content="a,&lt;b&gt;">"#
        );
    }
}
