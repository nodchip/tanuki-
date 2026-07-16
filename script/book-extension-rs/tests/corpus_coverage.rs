use book_extension_runtime::{
    corpus_coverage::generate_coverage_report,
    corpus_writer::{CorpusWriter, IngestContext, RecordOutcome},
    csa::parse_csa,
};
use tempfile::TempDir;

#[test]
fn reports_legal_distinct_book_coverage_without_loading_the_book() {
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    let game = parse_csa(
        b"V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
        "game.csa",
    )
    .unwrap();
    writer
        .ingest_batch(
            &IngestContext {
                site: "wcsc".to_owned(),
                event: "wcsc36".to_owned(),
                year: 2026,
                retrieved_at: 1.0,
                priority_key: [0; 88],
                now: 1.0,
            },
            vec![RecordOutcome::Accepted(game)],
        )
        .unwrap();
    let book = dir.path().join("book.db");
    std::fs::write(
        &book,
        "#YANEURAOU-DB2016 1.00\nsfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 99\n7g7f none 0 0 0\n7g7f none 0 0 0\n9a9b none 0 0 0\n",
    )
    .unwrap();

    let report =
        generate_coverage_report(writer.connection_mut(), &book, "pilot-001", "abc123").unwrap();

    assert_eq!(report.snapshot_id, "pilot-001");
    assert_eq!(report.book_hash, "abc123");
    assert_eq!(report.corpus_revision, 0);
    assert_eq!(report.overall.unique.covered, 1);
    assert_eq!(report.overall.unique.total, 2);
    assert_eq!(report.overall.unique.rate, 0.5);
    assert_eq!(report.overall.occurrences.covered, 1);
    assert!(report.by_site.contains_key("wcsc"));
    assert!(report.by_ply_band.contains_key("1-20"));
    assert!(report.by_side.contains_key("black"));
}
