#![no_main]
// ifc-lite end to end: scan, decode every entity, run its native geometry pipeline.
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    ifc_reader_spike::run_lite(data);
});
