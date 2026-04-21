#![no_main]

use libfuzzer_sys::fuzz_target;
use salvo_core::routing::{PathFilter, PathState};

fuzz_target!(|data: &[u8]| {
    if data.len() > 2048 {
        return;
    }

    let midpoint = data.len() / 2;
    let pattern = String::from_utf8_lossy(&data[..midpoint]);
    let request_path = salvo_fuzz::safe_uri_path(&data[midpoint..], 256);

    let filter = PathFilter::new(pattern.as_ref());
    let _ = PathFilter::try_new(pattern.as_ref());

    let mut state = PathState::new(&request_path);
    let _ = filter.detect(&mut state);
});
