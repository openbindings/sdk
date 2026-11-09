#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    openbindings_fuzz::json_value(data);
});
