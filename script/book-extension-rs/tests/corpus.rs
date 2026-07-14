use std::collections::HashSet;

use book_extension_runtime::{
    book::OpeningBook,
    coordinator::replay_corpus_results,
    corpus::{CorpusStore, SearchCompletion, SearchTaskStatus},
};

#[test]
fn creates_schema_v5_and_position_priority_index() {
    let directory = tempfile::tempdir().unwrap();
    let store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();

    assert_eq!(store.schema_version().unwrap(), 5);
    let plan = store
        .position_query_plan("snapshot", "position", 1)
        .unwrap();
    assert!(
        plan.iter()
            .any(|line| line.contains("candidate_position_priority_idx")),
        "query plan: {plan:?}"
    );
}

#[test]
fn rejects_schema_v4_with_rebuild_instruction() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corpus.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             INSERT INTO meta(key,value) VALUES('schema_version','4');",
        )
        .unwrap();
    drop(connection);

    let error = match CorpusStore::open(&path) {
        Ok(_) => panic!("schema v4 must be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("must be rebuilt as version 5"));
}
#[test]
fn reserves_only_highest_priority_candidate_at_visited_position() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &["7g7f"], "wcsc:event", "100")
        .unwrap();
    store
        .upsert_candidate("p", "2g2f", &["2g2f"], "denryu:event", "200")
        .unwrap();
    store
        .upsert_candidate("other", "8g8f", &[], "floodgate:event", "999")
        .unwrap();

    let candidate = store
        .reserve_for_position("book-hash", "p", &HashSet::new(), 1, 60.0, None, 1000.0)
        .unwrap()
        .expect("candidate reserved");

    assert_eq!(candidate.move_usi, "2g2f");
    assert_eq!(candidate.status, SearchTaskStatus::Running);
    assert_eq!(candidate.attempts, 1);
    assert_eq!(candidate.history, vec!["2g2f"]);
}

#[test]
fn completed_result_is_unpersisted_until_checkpoint_and_replays_for_unknown_hash() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    let task = store
        .reserve_for_position("book-a", "p", &HashSet::new(), 1, 60.0, None, 1000.0)
        .unwrap()
        .unwrap();
    store
        .complete_search(
            task.id,
            &SearchCompletion {
                eval_cp: 42,
                response: "3c3d".to_owned(),
                depth: 10,
                nodes: 1234,
                engine_config_id: "engine-a".to_owned(),
                now: 1001.0,
            },
        )
        .unwrap();

    assert_eq!(store.unpersisted_results().unwrap().len(), 1);
    let checkpoint = store.record_checkpoint("hash-a", 1002.0).unwrap();
    assert!(checkpoint > 0);
    assert!(store.unpersisted_results().unwrap().is_empty());
    assert!(store.results_requiring_replay("hash-a").unwrap().is_empty());
    let replay = store.results_requiring_replay("unknown-hash").unwrap();
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].move_usi, "7g7f");
    assert_eq!(replay[0].eval_cp, 42);
}

#[test]
fn failed_search_becomes_permanent_after_three_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    for attempt in 1..=3 {
        let task = store
            .reserve_for_position("book", "p", &HashSet::new(), 1, 60.0, None, attempt as f64)
            .unwrap()
            .unwrap();
        let status = store
            .fail_search(task.id, "engine failed", 3, attempt as f64 + 0.1)
            .unwrap();
        assert_eq!(
            status,
            if attempt == 3 {
                SearchTaskStatus::PermanentFailed
            } else {
                SearchTaskStatus::Pending
            }
        );
    }
    assert!(
        store
            .reserve_for_position("book", "p", &HashSet::new(), 1, 60.0, None, 10.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn resets_interrupted_running_tasks_to_pending() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    store
        .reserve_for_position("book", "p", &HashSet::new(), 1, 60.0, None, 1.0)
        .unwrap();

    assert_eq!(store.reset_interrupted_tasks(2.0).unwrap(), 1);
    assert!(
        store
            .reserve_for_position("book", "p", &HashSet::new(), 1, 60.0, None, 3.0)
            .unwrap()
            .is_some()
    );
}

#[test]
fn progressive_width_increments_after_saturation_and_revision_keeps_width() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    assert_eq!(store.progressive_width().unwrap(), 1);
    assert!(!store.record_corpus_rollout(false, 3, 1.0).unwrap());
    assert!(!store.record_corpus_rollout(false, 3, 2.0).unwrap());
    assert!(store.record_corpus_rollout(false, 3, 3.0).unwrap());
    assert_eq!(store.progressive_width().unwrap(), 2);
    assert_eq!(store.zero_addition_rollouts().unwrap(), 0);

    assert_eq!(store.bump_corpus_revision().unwrap(), 1);
    assert_eq!(store.progressive_width().unwrap(), 2);
    assert_eq!(store.zero_addition_rollouts().unwrap(), 0);
}

#[test]
fn replay_applies_evaluated_unpersisted_result_to_memory_book_once() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    let root = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1";
    store
        .upsert_candidate(root, "8g8f", &[], "wcsc:event", "100")
        .unwrap();
    let task = store
        .reserve_for_position("book", root, &HashSet::new(), 1, 60.0, None, 1.0)
        .unwrap()
        .unwrap();
    store
        .complete_search(
            task.id,
            &SearchCompletion {
                eval_cp: 66,
                response: "3c3d".to_owned(),
                depth: 8,
                nodes: 100,
                engine_config_id: "engine".to_owned(),
                now: 2.0,
            },
        )
        .unwrap();
    let mut book = OpeningBook::new(false);
    book.ensure_position(root).unwrap();

    assert_eq!(
        replay_corpus_results(&mut book, &store, "unknown").unwrap(),
        1
    );
    assert_eq!(
        replay_corpus_results(&mut book, &store, "unknown").unwrap(),
        0
    );
    assert_eq!(book.position(root).unwrap().entries[0].move_usi, "8g8f");
    assert_eq!(book.position(root).unwrap().entries[0].eval_cp, 66);
}
