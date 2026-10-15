#![no_main]
// Our reader end to end: index, decode every instance, mesh every element (booleans through Manifold).
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    ifc_reader_spike::run_own(data);
});
