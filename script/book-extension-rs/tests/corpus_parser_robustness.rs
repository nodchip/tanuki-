use std::panic::{AssertUnwindSafe, catch_unwind};

use book_extension_runtime::{csa::parse_csa, kif::parse_kif};

fn deterministic_bytes(case: u64) -> Vec<u8> {
    let length = (case as usize * 37) % 4097;
    let mut state = case ^ 0x9e37_79b9_7f4a_7c15;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn arbitrary_and_truncated_bytes_never_panic_or_abort() {
    for case in 0..2_000_u64 {
        let bytes = deterministic_bytes(case);
        let csa = catch_unwind(AssertUnwindSafe(|| parse_csa(&bytes, "fuzz.csa")));
        assert!(
            csa.is_ok(),
            "CSA parser panicked for deterministic case {case}"
        );
        let kif = catch_unwind(AssertUnwindSafe(|| parse_kif(&bytes, "fuzz.kif")));
        assert!(
            kif.is_ok(),
            "KIF parser panicked for deterministic case {case}"
        );

        if !bytes.is_empty() {
            let cut = bytes.len() / 2;
            let csa = catch_unwind(AssertUnwindSafe(|| parse_csa(&bytes[..cut], "cut.csa")));
            assert!(csa.is_ok(), "CSA parser panicked for truncated case {case}");
            let kif = catch_unwind(AssertUnwindSafe(|| parse_kif(&bytes[..cut], "cut.kif")));
            assert!(kif.is_ok(), "KIF parser panicked for truncated case {case}");
        }
    }
}
