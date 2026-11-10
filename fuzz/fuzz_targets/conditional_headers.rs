#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| salvo_fuzz::properties::conditional_headers(data));
