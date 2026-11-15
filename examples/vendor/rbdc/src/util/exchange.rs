/// impl exchange - optimized with byte iteration and batch copying
pub fn impl_exchange(start_str: &str, start_num: usize, sql: &str) -> String {
    let mut result = String::with_capacity(sql.len() * 3 / 2);
    let mut placeholder_idx = start_num;
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut start = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'?' {
            // Copy preceding chars in one batch
            if start < i {
                result.push_str(&sql[start..i]);
            }
            i += 2; // skip \?
            start = i;
            result.push('?');
        } else if bytes[i] == b'?' {
            // Copy preceding chars in one batch
            if start < i {
                result.push_str(&sql[start..i]);
            }
            i += 1;
            start = i;
            result.push_str(start_str);
            result.push_str(itoa::Buffer::new().format(placeholder_idx));
            placeholder_idx += 1;
        } else {
            i += 1;
        }
    }

    // Copy remaining chars
    if start < bytes.len() {
        result.push_str(&sql[start..]);
    }

    result
}
