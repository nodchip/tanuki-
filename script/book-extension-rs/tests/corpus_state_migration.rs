use book_extension_runtime::{
    corpus::{CorpusStore, SearchCompletion},
    corpus_state_migration::{MigrationIdentity, migrate_runtime_state},
};
use std::collections::HashSet;

fn identity() -> MigrationIdentity {
    MigrationIdentity {
        manifest_sha256: "11".repeat(32),
        profile_sha256: "22".repeat(32),
        priority_reference_year: 2026,
        priority_policy_version: "corpus-priority-v2".to_owned(),
        ranking_digest: "33".repeat(32),
        rating_digest: "44".repeat(32),
    }
}

#[test]
fn migrates_tasks_by_natural_key_when_candidate_ids_change() {
    let directory = tempfile::tempdir().unwrap();
    let old_path = directory.path().join("old.sqlite");
    let new_path = directory.path().join("new.sqlite");
    let mut old = CorpusStore::open(&old_path).unwrap();
    old.upsert_candidate("p", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    let task = old
        .reserve_for_position("book", "p", &HashSet::new(), 1, 60.0, None, 1.0)
        .unwrap()
        .unwrap();
    old.complete_search(
        task.id,
        &SearchCompletion {
            eval_cp: 42,
            response: "3c3d".to_owned(),
            depth: 10,
            nodes: 100,
            engine_config_id: "fingerprint".to_owned(),
            now: 2.0,
        },
    )
    .unwrap();
    old.add_metric("corpus_nodes:wcsc", 100).unwrap();
    old.record_checkpoint_through("book-hash", Some(task.id), 2.5)
        .unwrap();
    drop(old);

    let old_connection = rusqlite::Connection::open(&old_path).unwrap();
    old_connection
        .execute(
            "INSERT INTO frontier_history(old_band,new_band,old_width,new_width,changed_at,reason)
             VALUES(0,0,1,2,2.75,'v6 widening')",
            [],
        )
        .unwrap();
    drop(old_connection);

    let mut new = CorpusStore::open(&new_path).unwrap();
    new.upsert_candidate("other", "2g2f", &[], "wcsc:event", "999")
        .unwrap();
    new.upsert_candidate("p", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    drop(new);

    let summary = migrate_runtime_state(&new_path, &old_path, &identity(), 3.0).unwrap();

    assert_eq!(summary.source_tasks, 1);
    assert_eq!(summary.mapped_tasks, 1);
    assert_eq!(summary.unmapped_tasks, 0);
    assert_eq!(summary.copied_checkpoints, 1);
    assert_eq!(summary.copied_frontier_history, 1);
    let connection = rusqlite::Connection::open(&new_path).unwrap();
    let actual: (String, i64, i32) = connection
        .query_row(
            "SELECT t.status,t.attempts,t.eval_cp FROM search_task t
             JOIN candidate c ON c.id=t.candidate_id
             JOIN position p ON p.id=c.position_id
             WHERE p.position_key='p' AND c.move='7g7f' AND t.book_snapshot_id='book'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(actual, ("evaluated".to_owned(), 1, 42));
    let metric: i64 = connection
        .query_row(
            "SELECT value FROM metric_counter WHERE name='corpus_nodes:wcsc'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(metric, 100);
}

#[test]
fn migrates_schema_v5_tasks_checkpoints_metrics_and_width_history() {
    let directory = tempfile::tempdir().unwrap();
    let old_path = directory.path().join("old-v5.sqlite");
    let new_path = directory.path().join("new-v6.sqlite");
    let old = rusqlite::Connection::open(&old_path).unwrap();
    old.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         INSERT INTO meta VALUES('schema_version','5');
         INSERT INTO meta VALUES('progressive_width','3');
         INSERT INTO meta VALUES('zero_addition_rollouts','7');
         CREATE TABLE position(id INTEGER PRIMARY KEY,position_key TEXT NOT NULL UNIQUE);
         CREATE TABLE candidate(id INTEGER PRIMARY KEY,position_id INTEGER NOT NULL,move TEXT NOT NULL);
         CREATE TABLE search_task(
             id INTEGER PRIMARY KEY,candidate_id INTEGER NOT NULL,book_snapshot_id TEXT NOT NULL,
             status TEXT NOT NULL,attempts INTEGER NOT NULL,lease_until REAL,last_error TEXT,
             eval_cp INTEGER,response TEXT,depth INTEGER,nodes INTEGER,engine_config_id TEXT,
             persisted_checkpoint_id INTEGER,updated_at REAL NOT NULL
         );
         CREATE TABLE metric_counter(name TEXT PRIMARY KEY,value INTEGER NOT NULL);
         CREATE TABLE checkpoint(id INTEGER PRIMARY KEY,book_hash TEXT NOT NULL,created_at REAL NOT NULL);
         CREATE TABLE progressive_width_history(
             id INTEGER PRIMARY KEY,old_width INTEGER NOT NULL,new_width INTEGER NOT NULL,
             changed_at REAL NOT NULL,reason TEXT NOT NULL
         );
         INSERT INTO position VALUES(1,'p');
         INSERT INTO candidate VALUES(10,1,'7g7f');
         INSERT INTO checkpoint VALUES(9,'book-hash',12.0);
         INSERT INTO search_task VALUES(
             20,10,'book','running',2,99.0,'transient',NULL,NULL,NULL,NULL,
             'legacy-engine',9,13.0
         );
         INSERT INTO metric_counter VALUES('corpus_nodes:wcsc',123);
         INSERT INTO progressive_width_history VALUES(1,2,3,11.0,'legacy widening');",
    )
    .unwrap();
    drop(old);

    let mut new = CorpusStore::open(&new_path).unwrap();
    new.upsert_candidate("p", "7g7f", &[], "wcsc:event", "100")
        .unwrap();
    drop(new);

    let summary = migrate_runtime_state(&new_path, &old_path, &identity(), 20.0).unwrap();

    assert_eq!(summary.source_tasks, 1);
    assert_eq!(summary.mapped_tasks, 1);
    assert_eq!(summary.unmapped_tasks, 0);
    assert_eq!(summary.copied_checkpoints, 1);
    assert_eq!(summary.copied_frontier_history, 1);
    assert!(summary.identity_changed);
    let connection = rusqlite::Connection::open(&new_path).unwrap();
    let task: (
        String,
        i64,
        Option<String>,
        Option<String>,
        i64,
        Option<i64>,
    ) = connection
        .query_row(
            "SELECT status,attempts,failure_class,engine_fingerprint,
                    consecutive_engine_failures,persisted_checkpoint_id
             FROM search_task",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        task,
        (
            "pending".to_owned(),
            2,
            None,
            Some("legacy-engine".to_owned()),
            0,
            Some(9)
        )
    );
    let checkpoint: (String, f64) = connection
        .query_row(
            "SELECT book_hash,created_at FROM checkpoint WHERE id=9",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(checkpoint, ("book-hash".to_owned(), 12.0));
    let history: (i64, i64, i64, i64, String) = connection
        .query_row(
            "SELECT old_band,new_band,old_width,new_width,reason
             FROM frontier_history WHERE reason='legacy widening'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(history, (0, 0, 2, 3, "legacy widening".to_owned()));
    let frontier = CorpusStore::open(&new_path)
        .unwrap()
        .frontier_state()
        .unwrap();
    assert_eq!(frontier.active_quality_band, 0);
    assert_eq!(frontier.progressive_width, 3);
    assert_eq!(frontier.eligible_miss_count, 0);
}
