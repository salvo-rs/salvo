use rbdc::util::impl_exchange;

#[test]
fn test_impl_exchange_basic() {
    let result = impl_exchange("$", 1, "SELECT * FROM t WHERE id = ?");
    assert_eq!(result, "SELECT * FROM t WHERE id = $1");
}

#[test]
fn test_impl_exchange_multiple_placeholders() {
    let result = impl_exchange("$", 1, "INSERT INTO t VALUES (?, ?, ?)");
    assert_eq!(result, "INSERT INTO t VALUES ($1, $2, $3)");
}

#[test]
fn test_impl_exchange_escape_sequence() {
    let result = impl_exchange("$", 1, "SELECT jsonb '{\"a\":1}' \\? ?");
    assert_eq!(result, "SELECT jsonb '{\"a\":1}' ? $1");
}

#[test]
fn test_impl_escape_only() {
    let result = impl_exchange("$", 1, "\\?");
    assert_eq!(result, "?");
}

#[test]
fn test_impl_escape_with_placeholder() {
    let result = impl_exchange("$", 1, "\\? ?");
    assert_eq!(result, "? $1");
}

#[test]
fn test_impl_exchange_no_placeholders() {
    let result = impl_exchange("$", 1, "SELECT * FROM users");
    assert_eq!(result, "SELECT * FROM users");
}

#[test]
fn test_impl_exchange_start_num() {
    let result = impl_exchange("$", 5, "SELECT * FROM t WHERE id = ?");
    assert_eq!(result, "SELECT * FROM t WHERE id = $5");

    let result = impl_exchange("$", 5, "VALUES (?, ?, ?)");
    assert_eq!(result, "VALUES ($5, $6, $7)");
}

#[test]
fn test_impl_exchange_different_prefix() {
    let result = impl_exchange(":", 1, "SELECT * FROM t WHERE id = ?");
    assert_eq!(result, "SELECT * FROM t WHERE id = :1");
}

#[test]
fn test_impl_exchange_complex_query() {
    let sql = "INSERT INTO users (id, name, email) VALUES (?, ?, ?) WHERE status = ?";
    let result = impl_exchange("$", 1, sql);
    assert_eq!(
        result,
        "INSERT INTO users (id, name, email) VALUES ($1, $2, $3) WHERE status = $4"
    );
}

#[test]
fn test_impl_exchange_multiple_escapes() {
    let result = impl_exchange("$", 1, "\\? \\? ?");
    assert_eq!(result, "? ? $1");
}

#[test]
fn test_impl_exchange_empty_string() {
    let result = impl_exchange("$", 1, "");
    assert_eq!(result, "");
}

#[test]
fn test_impl_exchange_only_placeholder() {
    let result = impl_exchange("$", 1, "?");
    assert_eq!(result, "$1");
}

#[test]
fn test_impl_exchange_backslash_not_escape() {
    let result = impl_exchange("$", 1, "SELECT * FROM t WHERE path = '\\'");
    assert_eq!(result, "SELECT * FROM t WHERE path = '\\'");
}
