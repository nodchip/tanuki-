use std::{collections::BTreeSet, fs, path::Path};

use book_extension_runtime::corpus_build_profile::{
    ArchiveKind, CorpusBuildProfile, CorpusManifest,
};

fn repo_path(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn production_manifest_uses_only_the_audited_archive_formats() {
    let manifest = CorpusManifest::load(&repo_path("config/book-corpus/production-manifest.json"))
        .expect("production manifest must load");
    let kinds = manifest
        .sources
        .iter()
        .map(|source| ArchiveKind::from_relative_path(&source.relative_path))
        .collect::<Result<BTreeSet<_>, _>>()
        .expect("every production source must have a supported archive kind");

    assert_eq!(
        kinds,
        BTreeSet::from([
            ArchiveKind::SevenZip,
            ArchiveKind::TarXz,
            ArchiveKind::Zip,
            ArchiveKind::Lzh,
        ])
    );
}

#[test]
fn every_profile_input_matches_exactly_one_manifest_source() {
    for profile_relative in ["config/corpus-pilot.json", "config/corpus-production.json"] {
        let profile_path = repo_path(profile_relative);
        let profile = CorpusBuildProfile::load(&profile_path).expect("profile must load");
        let manifest = CorpusManifest::load(&profile.manifest).expect("manifest must load");

        for ingest in &profile.ingests {
            for input in &ingest.inputs {
                let matches = manifest
                    .sources
                    .iter()
                    .filter(|source| {
                        source.relative_path == *input
                            && source.site == ingest.site
                            && source.event == ingest.event
                            && source.year == ingest.year
                            && source.retrieved_at == ingest.retrieved_at
                    })
                    .count();
                assert_eq!(matches, 1, "{profile_relative}: {input}");
            }
        }
    }
}

#[test]
fn manifest_json_does_not_hide_unclassified_sources() {
    let text = fs::read_to_string(repo_path("config/book-corpus/production-manifest.json"))
        .expect("manifest text");
    let value: serde_json::Value = serde_json::from_str(&text).expect("manifest JSON");
    let source_count = value["sources"].as_array().expect("sources array").len();
    let manifest = CorpusManifest::load(&repo_path("config/book-corpus/production-manifest.json"))
        .expect("typed manifest");
    assert_eq!(manifest.sources.len(), source_count);
}
