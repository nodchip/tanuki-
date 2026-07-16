use std::collections::BTreeMap;

use book_extension_runtime::engine_fingerprint::{
    EngineFingerprintInput, compute_engine_fingerprint,
};

fn input(extra: Vec<(&str, &str)>, revision: u64) -> EngineFingerprintInput {
    EngineFingerprintInput {
        engine_sha256: "11".repeat(32),
        hash_mb: 1024,
        threads: 2,
        multipv: 4,
        extra: extra
            .into_iter()
            .map(|(name, value)| (name.to_owned(), Some(value.to_owned())))
            .collect::<BTreeMap<_, _>>(),
        corpus_nodes: 3000,
        search_timeout_sec: 60.0,
        engine_config_revision: revision,
    }
}

#[test]
fn fingerprint_normalizes_option_order_and_honors_revision() {
    let a = compute_engine_fingerprint(&input(vec![("A", "1"), ("B", "2")], 1)).unwrap();
    let b = compute_engine_fingerprint(&input(vec![("B", "2"), ("A", "1")], 1)).unwrap();
    let c = compute_engine_fingerprint(&input(vec![("A", "1"), ("B", "2")], 2)).unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
}
