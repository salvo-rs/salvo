//! Width limits of character-class path parameters follow the routing docs.
#![cfg(feature = "test")]

use salvo_core::prelude::*;
use salvo_core::test::TestClient;

#[handler]
async fn matched() {}

/// Returns the widths between 1 and 12 that the pattern accepts.
async fn accepted_widths(pattern: &str, digit: &str) -> Vec<usize> {
    let service = Service::new(Router::with_path(format!("item/{pattern}")).get(matched));
    let mut widths = Vec::new();
    for width in 1..=12 {
        let response = TestClient::get(format!("http://localhost/item/{}", digit.repeat(width)))
            .send(&service)
            .await;
        if response.status_code == Some(StatusCode::OK) {
            widths.push(width);
        }
    }
    widths
}

#[tokio::test]
async fn a_single_width_is_exact() {
    assert_eq!(accepted_widths("{id:num[6]}", "7").await, [6]);
    assert_eq!(accepted_widths("{id:hex[6]}", "f").await, [6]);
    assert_eq!(accepted_widths("{id:num[1]}", "7").await, [1]);
}

#[tokio::test]
async fn ranges_keep_their_documented_bounds() {
    for (pattern, expected) in [
        ("{id:num}", (1..=12).collect::<Vec<_>>()),
        ("{id:num(..4)}", vec![1, 2, 3]),
        ("{id:num(3..5)}", vec![3, 4]),
        ("{id:num(..=4)}", vec![1, 2, 3, 4]),
        ("{id:num(3..=5)}", vec![3, 4, 5]),
        ("{id:num(10..)}", vec![10, 11, 12]),
    ] {
        assert_eq!(accepted_widths(pattern, "7").await, expected, "{pattern}");
    }
}
