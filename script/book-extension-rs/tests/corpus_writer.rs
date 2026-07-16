use book_extension_runtime::{
    corpus_writer::{CorpusWriter, IngestContext, RecordOutcome},
    csa::parse_csa,
    record::{RecordError, RecordErrorKind},
};
use tempfile::TempDir;

fn count(writer: &CorpusWriter, table: &str) -> i64 {
    writer
        .connection()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn ingests_one_normalized_game_into_schema_v6() {
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    let game = parse_csa(
        b"V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
        "game.csa",
    )
    .unwrap();
    let context = IngestContext {
        site: "wcsc".to_owned(),
        event: "wcsc36".to_owned(),
        year: 2026,
        retrieved_at: 1783900800.0,
        priority_key: [7_u8; 88],
        now: 1783900900.0,
    };

    let result = writer
        .ingest_batch(&context, vec![RecordOutcome::Accepted(game)])
        .unwrap();

    assert_eq!(result.accepted, 1);
    assert_eq!(result.excluded, 0);
    assert_eq!(count(&writer, "raw_source"), 1);
    assert_eq!(count(&writer, "logical_game"), 1);
    assert_eq!(count(&writer, "source_game"), 1);
    assert_eq!(count(&writer, "position"), 2);
    assert_eq!(count(&writer, "game_position"), 2);
    assert_eq!(count(&writer, "candidate"), 2);
    let (length, references, band, source_site, recent, occurrences): (i64, i64, i64, i64, i64, i64) = writer
        .connection()
        .query_row(
            "SELECT length(priority_key), COUNT(representative_game_id), quality_band, source_site, recent_occurrences, occurrences FROM candidate",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .unwrap();
    assert_eq!(length, 88);
    assert_eq!(references, 2);
    assert_eq!((band, source_site, recent, occurrences), (4, 0, 0, 0));
    let version: String = writer
        .connection()
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, "6");
}

fn context(priority_byte: u8) -> IngestContext {
    IngestContext {
        site: "wcsc".to_owned(),
        event: "wcsc36".to_owned(),
        year: 2026,
        retrieved_at: 1783900800.0,
        priority_key: [priority_byte; 88],
        now: 1783900900.0,
    }
}

fn game(path: &str) -> book_extension_runtime::record::NormalizedGame {
    parse_csa(
        b"V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
        path,
    )
    .unwrap()
}

#[test]
fn preserves_mirror_sources_and_updates_representative_only_for_higher_priority() {
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    writer
        .ingest_batch(&context(7), vec![RecordOutcome::Accepted(game("a.csa"))])
        .unwrap();
    writer
        .ingest_batch(&context(6), vec![RecordOutcome::Accepted(game("b.csa"))])
        .unwrap();

    assert_eq!(count(&writer, "raw_source"), 2);
    assert_eq!(count(&writer, "logical_game"), 1);
    assert_eq!(count(&writer, "source_game"), 2);
    let source_before: String = writer
        .connection()
        .query_row(
            "SELECT rs.relative_path FROM candidate c JOIN raw_source rs ON rs.id=c.source_id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_before, "a.csa");

    let mut higher = game("c.csa");
    higher.players[0] = "Different Black".to_owned();
    writer
        .ingest_batch(&context(8), vec![RecordOutcome::Accepted(higher)])
        .unwrap();
    let source_after: String = writer
        .connection()
        .query_row(
            "SELECT rs.relative_path FROM candidate c JOIN raw_source rs ON rs.id=c.source_id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_after, "c.csa");
}

#[test]
fn excluded_records_are_idempotent_and_failed_batches_roll_back() {
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    let error = RecordError::new(
        "broken.csa",
        RecordErrorKind::TimeBeforeFirstMove,
        Some(4),
        "time before move",
    );
    let outcome = || RecordOutcome::Excluded {
        relative_path: "broken.csa".to_owned(),
        sha256: "abc".to_owned(),
        error: error.clone(),
    };
    writer
        .ingest_batch(&context(1), vec![outcome(), outcome()])
        .unwrap();
    assert_eq!(count(&writer, "raw_source"), 1);
    assert_eq!(count(&writer, "ingest_error"), 1);

    let mut invalid = game("invalid.csa");
    invalid.positions.clear();
    assert!(
        writer
            .ingest_batch(&context(1), vec![RecordOutcome::Accepted(invalid)])
            .is_err()
    );
    assert_eq!(count(&writer, "raw_source"), 1);
    assert_eq!(count(&writer, "logical_game"), 0);
}
