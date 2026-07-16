use std::{fs, path::Path};

use book_extension_runtime::corpus_build_profile::{CorpusBuildProfile, CorpusManifest};
use tempfile::TempDir;

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn valid_manifest() -> &'static str {
    r#"{
      "manifest_version": 1,
      "created_at_utc": "2026-07-15T00:00:00Z",
      "sources": [{
        "site": "wcsc", "event": "wcsc36", "year": 2026,
        "retrieved_at": 1783900800, "url": "https://example.invalid/wcsc36.zip",
        "relative_path": "wcsc36.zip", "size": 10,
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
      }]
    }"#
}

fn valid_profile(extra: &str) -> String {
    format!(
        r#"{{
          "schema_version": 1,
          "name": "pilot",
          "priority_reference_year": 2026,
          "manifest": "manifest.json",
          "ingests": [{{
            "site": "wcsc", "event": "wcsc36", "year": 2026,
            "retrieved_at": 1783900800, "inputs": ["wcsc36.zip"]
          }}],
          "ranking_files": [],
          "alias_files": [],
          "rating_file": null,
          "rating_config": null,
          "input_book": null,
          "coverage_snapshot_id": "pilot-initial"{extra}
        }}"#
    )
}

#[test]
fn rejects_unknown_profile_keys() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    write(
        &dir.path().join("profile.json"),
        &valid_profile(",\n\"surprise\": true"),
    );

    let error = CorpusBuildProfile::load(&dir.path().join("profile.json")).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
}

#[test]
fn rejects_unsupported_profile_schema_version() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    let profile = valid_profile("").replace("\"schema_version\": 1", "\"schema_version\": 2");
    write(&dir.path().join("profile.json"), &profile);

    let error = CorpusBuildProfile::load(&dir.path().join("profile.json")).unwrap_err();
    assert!(error.to_string().contains("schema version"), "{error}");
}

#[test]
fn rejects_duplicate_manifest_relative_paths() {
    let dir = TempDir::new().unwrap();
    let duplicate = valid_manifest().replace(
        "}]\n    }",
        "}, {\"site\":\"wcsc\",\"event\":\"wcsc36\",\"year\":2026,\"retrieved_at\":1783900800,\"url\":\"https://example.invalid/copy\",\"relative_path\":\"wcsc36.zip\",\"size\":10,\"sha256\":\"1111111111111111111111111111111111111111111111111111111111111111\"}]\n    }",
    );
    let path = dir.path().join("manifest.json");
    write(&path, &duplicate);

    let error = CorpusManifest::load(&path).unwrap_err();
    assert!(
        error.to_string().contains("duplicate manifest path"),
        "{error}"
    );
}

#[test]
fn rejects_ingest_metadata_that_does_not_match_manifest() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    let profile = valid_profile("").replace("\"year\": 2026", "\"year\": 2025");
    write(&dir.path().join("profile.json"), &profile);

    let error = CorpusBuildProfile::load(&dir.path().join("profile.json")).unwrap_err();
    assert!(
        error.to_string().contains("does not match manifest"),
        "{error}"
    );
}

#[test]
fn rejects_parent_directory_in_manifest_source_path() {
    let dir = TempDir::new().unwrap();
    let manifest = valid_manifest().replace("wcsc36.zip", "../wcsc36.zip");
    let path = dir.path().join("manifest.json");
    write(&path, &manifest);

    let error = CorpusManifest::load(&path).unwrap_err();
    assert!(
        error.to_string().contains("unsafe relative path"),
        "{error}"
    );
}

#[test]
fn rating_file_and_policy_must_be_configured_together() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    let profile =
        valid_profile("").replace("\"rating_file\": null", "\"rating_file\": \"rating.json\"");
    let path = dir.path().join("profile.json");
    write(&path, &profile);

    let error = CorpusBuildProfile::load(&path).unwrap_err();
    assert!(error.to_string().contains("rating_file and rating_config"));
}

#[test]
fn rejects_invalid_member_pattern_before_download() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    let profile = valid_profile("").replace(
        "\"inputs\": [\"wcsc36.zip\"]",
        "\"inputs\": [\"wcsc36.zip\"], \"member_pattern\": \"[\"",
    );
    let path = dir.path().join("profile.json");
    write(&path, &profile);

    let error = CorpusBuildProfile::load(&path).unwrap_err();
    assert!(error.to_string().contains("member_pattern"), "{error}");
}

#[test]
fn rejects_non_http_manifest_url() {
    let dir = TempDir::new().unwrap();
    let manifest =
        valid_manifest().replace("https://example.invalid/wcsc36.zip", "file:///wcsc36.zip");
    let path = dir.path().join("manifest.json");
    write(&path, &manifest);

    let error = CorpusManifest::load(&path).unwrap_err();
    assert!(
        error.to_string().contains("invalid manifest source"),
        "{error}"
    );
}

#[test]
fn rejects_missing_metadata_file_before_download() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    let profile = valid_profile("").replace(
        "\"ranking_files\": []",
        "\"ranking_files\": [\"missing-ranking.json\"]",
    );
    let path = dir.path().join("profile.json");
    write(&path, &profile);

    let error = CorpusBuildProfile::load(&path).unwrap_err();
    assert!(error.to_string().contains("metadata file"), "{error}");
}

#[test]
fn loads_priority_reference_year_and_alias_files() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    write(&dir.path().join("aliases.json"), "{}");
    let profile =
        valid_profile("").replace("\"alias_files\": []", "\"alias_files\": [\"aliases.json\"]");
    let path = dir.path().join("profile.json");
    write(&path, &profile);

    let loaded = CorpusBuildProfile::load(&path).unwrap();
    assert_eq!(loaded.priority_reference_year, 2026);
    assert_eq!(loaded.alias_files, vec![dir.path().join("aliases.json")]);
}

#[test]
fn rejects_non_positive_priority_reference_year() {
    let dir = TempDir::new().unwrap();
    write(&dir.path().join("manifest.json"), valid_manifest());
    let profile = valid_profile("").replace(
        "\"priority_reference_year\": 2026",
        "\"priority_reference_year\": 0",
    );
    let path = dir.path().join("profile.json");
    write(&path, &profile);

    let error = CorpusBuildProfile::load(&path).unwrap_err();
    assert!(
        error.to_string().contains("priority_reference_year"),
        "{error}"
    );
}
