use std::collections::HashSet;

use book_extension_runtime::{
    book::OpeningBook,
    coordinator::replay_corpus_results,
    corpus::{
        CorpusStore, FailureClass, FrontierTransition, RolloutObservation, SearchCompletion,
        SearchTaskStatus, SourceSite,
    },
};

#[test]
fn creates_schema_v6_and_position_site_quality_index() {
    let directory = tempfile::tempdir().unwrap();
    let store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();

    assert_eq!(store.schema_version().unwrap(), 6);
    let plan = store
        .position_query_plan("snapshot", "position", 0, SourceSite::Wcsc, 1)
        .unwrap();
    assert!(
        plan.iter()
            .any(|line| line.contains("candidate_position_site_quality_idx")),
        "query plan: {plan:?}"
    );
}

#[test]
fn rejects_schema_v5_with_rebuild_instruction() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corpus.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             INSERT INTO meta(key,value) VALUES('schema_version','5');",
        )
        .unwrap();
    drop(connection);

    let error = match CorpusStore::open(&path) {
        Ok(_) => panic!("schema v5 must be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("must be rebuilt as version 6"));
}
#[test]
fn schema_v6_contains_frontier_and_failure_state() {
    let directory = tempfile::tempdir().unwrap();
    let store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();

    for column in [
        "quality_band",
        "source_site",
        "recent_occurrences",
        "occurrences",
    ] {
        let present: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('candidate') WHERE name=?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(present, 1, "missing candidate.{column}");
    }
    for column in [
        "failure_class",
        "engine_fingerprint",
        "consecutive_engine_failures",
    ] {
        let present: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('search_task') WHERE name=?1",
                [column],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(present, 1, "missing search_task.{column}");
    }
    let frontier_table: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='frontier_history'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(frontier_table, 1);
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

#[test]
fn probe_filters_site_and_band_before_width_and_observes_next_band() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &[], "wcsc:event", "300")
        .unwrap();
    store
        .upsert_candidate("p", "2g2f", &[], "wcsc:event", "200")
        .unwrap();
    store
        .upsert_candidate("p", "8g8f", &[], "denryu:event", "999")
        .unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    connection
        .execute(
            "UPDATE candidate SET source_site=?1,quality_band=0 WHERE move IN ('7g7f','2g2f')",
            [SourceSite::Wcsc as i32],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE candidate SET source_site=?1,quality_band=1 WHERE move='8g8f'",
            [SourceSite::Denryu as i32],
        )
        .unwrap();
    drop(connection);

    let probe = store
        .probe_position("book", "p", &HashSet::new(), 0, SourceSite::Wcsc, 1, 1000.0)
        .unwrap();

    assert_eq!(probe.choices.len(), 1);
    assert_eq!(probe.choices[0].move_usi, "7g7f");
    assert_eq!(probe.choices[0].quality_band, 0);
    assert_eq!(probe.choices[0].source_site, SourceSite::Wcsc);
    assert!(probe.truncated);
    assert_eq!(probe.next_quality_band, Some(1));
}

#[test]
fn probe_uses_n_plus_one_only_for_truncation_and_requires_width_growth_for_lower_rank() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &[], "wcsc:event", "300")
        .unwrap();
    store
        .upsert_candidate("p", "2g2f", &[], "wcsc:event", "200")
        .unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    connection
        .execute(
            "UPDATE candidate SET source_site=?1,quality_band=0",
            [SourceSite::Wcsc as i32],
        )
        .unwrap();
    drop(connection);
    let excluded = HashSet::from(["7g7f".to_owned()]);

    let width_one = store
        .probe_position("book", "p", &excluded, 0, SourceSite::Wcsc, 1, 1000.0)
        .unwrap();
    assert!(width_one.choices.is_empty());
    assert!(width_one.truncated);

    let width_two = store
        .probe_position("book", "p", &excluded, 0, SourceSite::Wcsc, 2, 1000.0)
        .unwrap();
    assert_eq!(width_two.choices.len(), 1);
    assert_eq!(width_two.choices[0].move_usi, "2g2f");
    assert!(!width_two.truncated);
}
#[test]
fn reserve_choice_rejects_stale_probe_without_incrementing_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .upsert_candidate("p", "7g7f", &[], "wcsc:event", "300")
        .unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    connection
        .execute(
            "UPDATE candidate SET source_site=?1,quality_band=0 WHERE move='7g7f'",
            [SourceSite::Wcsc as i32],
        )
        .unwrap();
    drop(connection);
    let choice = store
        .probe_position("book", "p", &HashSet::new(), 0, SourceSite::Wcsc, 1, 1000.0)
        .unwrap()
        .choices
        .into_iter()
        .next()
        .unwrap();

    let reserved = store
        .reserve_choice(&choice, "book", 60.0, 1000.0)
        .unwrap()
        .unwrap();
    assert_eq!(reserved.attempts, 1);
    assert!(
        store
            .reserve_choice(&choice, "book", 60.0, 1001.0)
            .unwrap()
            .is_none()
    );
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    let attempts: i64 = connection
        .query_row("SELECT attempts FROM search_task", [], |row| row.get(0))
        .unwrap();
    assert_eq!(attempts, 1);
}

#[test]
fn frontier_relaxes_width_then_band_and_persists_across_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("corpus.sqlite");
    let mut store = CorpusStore::open(&path).unwrap();
    for miss in 1..100 {
        assert_eq!(
            store
                .record_rollout_observation(
                    &RolloutObservation {
                        eligible_miss: true,
                        reserved: false,
                        truncated_in_active_band: miss == 1,
                        smallest_higher_band: Some(3),
                    },
                    100,
                    miss as f64,
                )
                .unwrap(),
            FrontierTransition::None
        );
    }
    assert_eq!(store.frontier_state().unwrap().eligible_miss_count, 99);
    assert_eq!(
        store
            .record_rollout_observation(
                &RolloutObservation {
                    eligible_miss: true,
                    reserved: false,
                    truncated_in_active_band: false,
                    smallest_higher_band: Some(3),
                },
                100,
                100.0,
            )
            .unwrap(),
        FrontierTransition::Width { old: 1, new: 2 }
    );
    drop(store);

    let mut store = CorpusStore::open(&path).unwrap();
    let state = store.frontier_state().unwrap();
    assert_eq!(state.active_quality_band, 0);
    assert_eq!(state.progressive_width, 2);
    for miss in 1..=100 {
        let transition = store
            .record_rollout_observation(
                &RolloutObservation {
                    eligible_miss: true,
                    reserved: false,
                    truncated_in_active_band: false,
                    smallest_higher_band: Some(3),
                },
                100,
                100.0 + miss as f64,
            )
            .unwrap();
        assert_eq!(
            transition,
            if miss == 100 {
                FrontierTransition::Band { old: 0, new: 3 }
            } else {
                FrontierTransition::None
            }
        );
    }
    let state = store.frontier_state().unwrap();
    assert_eq!(state.active_quality_band, 3);
    assert_eq!(state.progressive_width, 2);
}

#[test]
fn reservation_resets_misses_and_only_active_frontier_tasks_block_relaxation() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    store
        .record_rollout_observation(
            &RolloutObservation {
                eligible_miss: true,
                reserved: false,
                truncated_in_active_band: false,
                smallest_higher_band: Some(3),
            },
            100,
            1.0,
        )
        .unwrap();
    store
        .record_rollout_observation(
            &RolloutObservation {
                eligible_miss: false,
                reserved: true,
                truncated_in_active_band: false,
                smallest_higher_band: None,
            },
            100,
            2.0,
        )
        .unwrap();
    assert_eq!(store.frontier_state().unwrap().eligible_miss_count, 0);

    store
        .upsert_candidate("closed", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    connection
        .execute("UPDATE candidate SET quality_band=3", [])
        .unwrap();
    drop(connection);
    store
        .reserve_for_position("book", "closed", &HashSet::new(), 1, 60.0, None, 3.0)
        .unwrap()
        .unwrap();
    for miss in 1..=100 {
        let transition = store
            .record_rollout_observation(
                &RolloutObservation {
                    eligible_miss: true,
                    reserved: false,
                    truncated_in_active_band: false,
                    smallest_higher_band: Some(3),
                },
                100,
                3.0 + miss as f64,
            )
            .unwrap();
        assert_eq!(
            transition,
            if miss == 100 {
                FrontierTransition::Band { old: 0, new: 3 }
            } else {
                FrontierTransition::None
            }
        );
    }
}

#[test]
fn revision_reset_preserves_width_and_clears_frontier_observations() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    for miss in 1..=100 {
        store
            .record_rollout_observation(
                &RolloutObservation {
                    eligible_miss: true,
                    reserved: false,
                    truncated_in_active_band: true,
                    smallest_higher_band: Some(2),
                },
                100,
                miss as f64,
            )
            .unwrap();
    }
    store
        .reset_frontier_for_revision(101.0, "policy changed")
        .unwrap();
    let state = store.frontier_state().unwrap();
    assert_eq!(state.active_quality_band, 0);
    assert_eq!(state.progressive_width, 2);
    assert_eq!(state.eligible_miss_count, 0);
    assert!(!state.truncated_candidate_seen);
    assert_eq!(state.next_quality_band_seen, None);
}

#[test]
fn bump_revision_uses_frontier_reset_contract() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CorpusStore::open(&directory.path().join("corpus.sqlite")).unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    for (key, value) in [
        ("active_quality_band", "3"),
        ("progressive_width", "4"),
        ("eligible_miss_count", "9"),
        ("truncated_candidate_seen", "1"),
        ("next_quality_band_seen", "5"),
    ] {
        connection
            .execute("UPDATE meta SET value=?1 WHERE key=?2", [value, key])
            .unwrap();
    }
    drop(connection);

    assert_eq!(store.bump_corpus_revision().unwrap(), 1);

    let state = store.frontier_state().unwrap();
    assert_eq!(state.active_quality_band, 0);
    assert_eq!(state.progressive_width, 4);
    assert_eq!(state.eligible_miss_count, 0);
    assert!(!state.truncated_candidate_seen);
    assert_eq!(state.next_quality_band_seen, None);
}

#[test]
fn failure_classes_and_fingerprint_requeue_follow_policy() {
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
            .fail_engine_search(
                task.id,
                "engine failed",
                "fingerprint-a",
                attempt as f64 + 0.1,
            )
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
    assert_eq!(
        store
            .requeue_retryable_failures("fingerprint-a", 10.0)
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .requeue_retryable_failures("fingerprint-b", 11.0)
            .unwrap(),
        1
    );
    let task = store
        .reserve_for_position("book", "p", &HashSet::new(), 1, 60.0, None, 12.0)
        .unwrap()
        .unwrap();
    store.interrupt_search(task.id, 13.0).unwrap();
    let connection = rusqlite::Connection::open(store.path()).unwrap();
    let consecutive: i64 = connection
        .query_row(
            "SELECT consecutive_engine_failures FROM search_task WHERE id=?1",
            [task.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(consecutive, 0);

    store
        .upsert_candidate("q", "2g2f", &[], "wcsc:event", "100")
        .unwrap();
    let task = store
        .reserve_for_position("book", "q", &HashSet::new(), 1, 60.0, None, 14.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .fail_deterministic(
                task.id,
                FailureClass::InvalidCandidate,
                "illegal move",
                "fingerprint-b",
                15.0,
            )
            .unwrap(),
        SearchTaskStatus::PermanentFailed
    );
}
