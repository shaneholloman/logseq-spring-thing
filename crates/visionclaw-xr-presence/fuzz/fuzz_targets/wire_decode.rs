#![no_main]

//! Fuzz target for the avatar pose wire decoder (opcode 0x43).
//!
//! PRD-QE-002 §4.6 and Q5 require the binary protocol decoder to be fuzzed
//! nightly. The contract is: `decode(any &[u8]) -> Result<DecodedFrame, WireError>`
//! must be total — no panics, no UB, no out-of-bounds reads, regardless of
//! input. Any divergence here is a P1 release blocker per PRD-QE-002 §4.6.

use libfuzzer_sys::fuzz_target;
use visionclaw_xr_presence::wire::decode;

fuzz_target!(|data: &[u8]| {
    let _ = decode(data);
});
