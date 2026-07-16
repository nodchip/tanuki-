use book_extension_runtime::{
    corpus_metadata::{
        FloodgateRating, MetadataError, ParticipantAliasDocument, RatingPolicy, TournamentRanking,
        add_participant_alias, import_floodgate_rating, import_participant_aliases,
        import_tournament_ranking, recompute_candidate_priorities,
        recompute_candidate_priorities_with_control,
    },
    corpus_writer::{CorpusWriter, IngestContext, RecordOutcome},
    csa::parse_csa,
};
use tempfile::TempDir;

#[test]
fn imports_strict_ranking_and_recomputes_nfkc_matched_priority() {
    let ranking: TournamentRanking = serde_json::from_str(
        r#"{
          "site":"wcsc","event":"fixture","source_url":"fixture://ranking",
          "source_sha256":"1111111111111111111111111111111111111111111111111111111111111111",
          "retrieved_at":1000,"provisional":false,
          "results":[
            {"official_name":"Alpha","stage":"final","stage_tier":3,"rank":1,"participants":2},
            {"official_name":"Beta","stage":"lower","stage_tier":2,"rank":2,"participants":2}
          ]
        }"#,
    )
    .unwrap();
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    let mut game = parse_csa(
        b"V2.2\nN+Black\nN-White\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
        "game.csa",
    )
    .unwrap();
    game.players = ["Ａｌｐｈａ".to_owned(), "Beta".to_owned()];
    writer
        .ingest_batch(
            &IngestContext {
                site: "wcsc".to_owned(),
                event: "fixture".to_owned(),
                year: 2026,
                retrieved_at: 1000.0,
                priority_key: [0; 88],
                now: 1000.0,
            },
            vec![RecordOutcome::Accepted(game)],
        )
        .unwrap();

    import_tournament_ranking(writer.connection_mut(), &ranking).unwrap();
    assert_eq!(
        recompute_candidate_priorities(writer.connection_mut(), 2026).unwrap(),
        2
    );

    let actual: (i32, i32, i64, i64) = writer
        .connection()
        .query_row(
            "SELECT quality_band,source_site,recent_occurrences,occurrences
             FROM candidate c JOIN game_position gp
               ON gp.position_id=c.position_id AND gp.move=c.move WHERE gp.ply=0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(actual, (0, 0, 1, 1));
}

#[test]
fn mirrored_official_source_can_become_representative_without_double_counting() {
    let ranking: TournamentRanking = serde_json::from_str(
        r#"{
          "site":"wcsc","event":"fixture","source_url":"fixture://ranking",
          "source_sha256":"1111111111111111111111111111111111111111111111111111111111111111",
          "retrieved_at":1000,"provisional":false,
          "results":[
            {"official_name":"Alpha","stage":"final","stage_tier":1,"rank":1,"participants":2},
            {"official_name":"Beta","stage":"final","stage_tier":1,"rank":2,"participants":2}
          ]
        }"#,
    )
    .unwrap();
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    let mut game = parse_csa(
        b"V2.2\nN+Alpha\nN-Beta\nPI\n+\n+7776FU\n-3334FU\n%TORYO\n",
        "floodgate/game.csa",
    )
    .unwrap();
    writer
        .ingest_batch(
            &IngestContext {
                site: "floodgate".to_owned(),
                event: "floodgate-2026".to_owned(),
                year: 2026,
                retrieved_at: 900.0,
                priority_key: [0; 88],
                now: 900.0,
            },
            vec![RecordOutcome::Accepted(game.clone())],
        )
        .unwrap();
    game.source_path = "wcsc/game.csa".to_owned();
    game.sha256 = "22".repeat(32);
    writer
        .ingest_batch(
            &IngestContext {
                site: "wcsc".to_owned(),
                event: "fixture".to_owned(),
                year: 2026,
                retrieved_at: 1000.0,
                priority_key: [0; 88],
                now: 1000.0,
            },
            vec![RecordOutcome::Accepted(game)],
        )
        .unwrap();
    import_tournament_ranking(writer.connection_mut(), &ranking).unwrap();

    recompute_candidate_priorities(writer.connection_mut(), 2026).unwrap();

    let actual: (i32, i32, i64) = writer
        .connection()
        .query_row(
            "SELECT quality_band,source_site,occurrences FROM candidate ORDER BY id LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(actual, (0, 0, 1));
}
#[test]
fn ranking_json_rejects_unknown_fields() {
    let result = serde_json::from_str::<TournamentRanking>(
        r#"{"site":"wcsc","event":"e","source_url":"u","source_sha256":"11","retrieved_at":1,"provisional":false,"results":[],"surprise":true}"#,
    );
    assert!(result.is_err());
}

#[test]
fn explicit_alias_is_used_but_unlisted_names_are_not_fuzzy_matched() {
    let ranking: TournamentRanking = serde_json::from_str(
        r#"{
          "site":"wcsc","event":"fixture","source_url":"fixture://ranking",
          "source_sha256":"1111111111111111111111111111111111111111111111111111111111111111",
          "retrieved_at":1000,"provisional":false,
          "results":[
            {"official_name":"Official Alpha","stage":"final","stage_tier":3,"rank":1,"participants":1}
          ]
        }"#,
    )
    .unwrap();
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    import_tournament_ranking(writer.connection_mut(), &ranking).unwrap();
    add_participant_alias(
        writer.connection_mut(),
        "fixture",
        "Ａｌｉａｓ Alpha",
        "Official Alpha",
    )
    .unwrap();
    let mapping: (String, String) = writer
        .connection()
        .query_row(
            "SELECT alias,mapping_status FROM participant_alias",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(mapping, ("Alias Alpha".to_owned(), "manual".to_owned()));
}

#[test]
fn alias_document_rejects_conflicting_normalized_alias() {
    let ranking: TournamentRanking = serde_json::from_str(
        r#"{
          "site":"wcsc","event":"fixture","source_url":"fixture://ranking",
          "source_sha256":"1111111111111111111111111111111111111111111111111111111111111111",
          "retrieved_at":1000,"results":[
            {"official_name":"Alpha","stage":"final","stage_tier":1,"rank":1,"participants":2},
            {"official_name":"Beta","stage":"final","stage_tier":1,"rank":2,"participants":2}
          ]
        }"#,
    )
    .unwrap();
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();
    import_tournament_ranking(writer.connection_mut(), &ranking).unwrap();
    let first: ParticipantAliasDocument = serde_json::from_str(
        r#"{"event":"fixture","aliases":[{"alias":"Ａlias","official_name":"Alpha"}]}"#,
    )
    .unwrap();
    import_participant_aliases(writer.connection_mut(), &first).unwrap();
    let conflict: ParticipantAliasDocument = serde_json::from_str(
        r#"{"event":"fixture","aliases":[{"alias":"Alias","official_name":"Beta"}]}"#,
    )
    .unwrap();
    let error = import_participant_aliases(writer.connection_mut(), &conflict).unwrap_err();
    assert!(error.to_string().contains("alias collision"));
}

#[test]
fn floodgate_reliability_uses_connectivity_effective_games_and_component_quality() {
    let rating: FloodgateRating = serde_json::from_str(
        r#"{
          "year":2026,"snapshot_time":1000,"source_url":"fixture://rating",
          "source_sha256":"2222222222222222222222222222222222222222222222222222222222222222",
          "anchor_era":"2024-2026","rated_player_count":100,"component_size":50,
          "anchor_connected_rate":0.9,
          "results":[
            {"player_name":"Ａｌｐｈａ","rating":4000,"effective_games":50,
             "connected_to_anchor":true,"anchor_margin":25,"snapshot_percentile":0.9},
            {"player_name":"Beta","rating":3900,"effective_games":5,
             "connected_to_anchor":false,"anchor_margin":10,"snapshot_percentile":0.8}
          ]
        }"#,
    )
    .unwrap();
    let dir = TempDir::new().unwrap();
    let mut writer = CorpusWriter::create(&dir.path().join("corpus.sqlite")).unwrap();

    import_floodgate_rating(
        writer.connection_mut(),
        &rating,
        RatingPolicy {
            medium_games: 15.0,
            high_games: 50.0,
            min_component_size: 10,
        },
    )
    .unwrap();

    let rows: Vec<(String, i32)> = writer
        .connection()
        .prepare("SELECT player_name,reliability_bucket FROM rating_result ORDER BY player_name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, [("Alpha".to_owned(), 3), ("Beta".to_owned(), 1)]);
}

#[test]
fn priority_recompute_can_stop_between_pages_and_resume_without_publishing_temp_index() {
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
                event: "fixture".to_owned(),
                year: 2026,
                retrieved_at: 1000.0,
                priority_key: [0; 88],
                now: 1000.0,
            },
            vec![RecordOutcome::Accepted(game)],
        )
        .unwrap();

    let error = recompute_candidate_priorities_with_control(writer.connection_mut(), 2026, || true)
        .unwrap_err();
    assert!(matches!(error, MetadataError::Stopped));

    assert_eq!(
        recompute_candidate_priorities(writer.connection_mut(), 2026).unwrap(),
        2
    );
    let temporary_index: i64 = writer
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name IN ('game_position_candidate_lookup_idx','source_game_game_lookup_idx')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(temporary_index, 0);
}
