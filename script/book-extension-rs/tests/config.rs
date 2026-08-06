use std::path::Path;

use book_extension_runtime::config::{ConfigError, ExtensionConfig};

#[test]
fn loads_existing_pilot_configuration_and_expands_roles() {
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("config")
        .join("book-extension-pilot.toml");

    let config = ExtensionConfig::load(&config_path).expect("pilot config loads");

    assert_eq!(config.workers.engine_count, 8);
    assert_eq!(config.workers.threads_per_engine, 1);
    assert_eq!(
        config.workers.roles(),
        vec![
            "fixed_black",
            "fixed_black",
            "fixed_white",
            "fixed_white",
            "general",
            "general",
            "general",
            "general",
        ]
    );
    assert_eq!(config.corpus.max_concurrent_searches, 1);
    assert_eq!(config.runtime.backup_count, 3);
    assert_eq!(config.runtime.depth_histogram_interval_sec, 3600.0);
    assert!(config.runtime.state_dir.is_absolute());
}

#[test]
fn rejects_non_positive_depth_histogram_interval() {
    let error = ExtensionConfig::from_toml(
        r#"
[workers]
engine_count = 1
threads_per_engine = 1
fixed_black = 0
fixed_white = 0
general = 1
[corpus]
enabled = false
max_concurrent_searches = 0
general_pool_node_share = 0.0
[runtime]
state_dir = "C:/state"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 60
depth_histogram_interval_sec = 0
"#,
        Path::new("C:/config/book.toml"),
    )
    .expect_err("zero histogram interval rejected");

    assert!(
        error
            .to_string()
            .contains("depth_histogram_interval_sec must be positive")
    );
}

#[test]
fn rejects_role_sum_mismatch_without_adjustment() {
    let error = ExtensionConfig::from_toml(
        r#"
[workers]
engine_count = 8
threads_per_engine = 1
vulnerability_black = 2
vulnerability_white = 2
general = 3
[corpus]
enabled = true
max_concurrent_searches = 1
general_pool_node_share = 0.25
[runtime]
state_dir = "C:/state"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 60
"#,
        Path::new("C:/config/book.toml"),
    )
    .expect_err("invalid role sum rejected");

    assert!(matches!(error, ConfigError::Invalid(_)));
    assert!(error.to_string().contains("engine_count must equal"));
}

#[test]
fn rejects_enabled_corpus_concurrency_above_general_workers() {
    let error = ExtensionConfig::from_toml(
        r#"
[workers]
engine_count = 2
threads_per_engine = 1
vulnerability_black = 0
vulnerability_white = 0
general = 2
[corpus]
enabled = true
max_concurrent_searches = 3
general_pool_node_share = 0.25
[runtime]
state_dir = "state"
save_interval_sec = 3600
backup_count = 3
heartbeat_timeout_sec = 10
usi_stop_timeout_sec = 60
"#,
        Path::new("C:/config/book.toml"),
    )
    .expect_err("invalid concurrency rejected");

    assert!(error.to_string().contains("max_concurrent_searches"));
}
