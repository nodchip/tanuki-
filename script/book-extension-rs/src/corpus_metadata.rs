use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::Instant,
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::corpus::SourceSite;
use crate::priority::{
    FloodgateStrength, OfficialStrength, POLICY_VERSION, PriorityError, PriorityFacts,
    QualityFacts, encode_priority, floodgate_strength_tier, official_strength_tier, priority_tuple,
    quality_decision,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantAliasDocument {
    pub event: String,
    pub aliases: Vec<ParticipantAlias>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantAlias {
    pub alias: String,
    pub official_name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TournamentRanking {
    pub site: String,
    pub event: String,
    pub source_url: String,
    pub source_sha256: String,
    pub retrieved_at: f64,
    #[serde(default)]
    pub provisional: bool,
    pub results: Vec<TournamentResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TournamentResult {
    pub official_name: String,
    pub stage: String,
    pub stage_tier: i32,
    pub rank: i32,
    pub participants: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloodgateRating {
    pub year: i32,
    pub snapshot_time: f64,
    pub source_url: String,
    pub source_sha256: String,
    pub anchor_era: String,
    pub rated_player_count: i32,
    pub component_size: i32,
    pub anchor_connected_rate: f64,
    pub results: Vec<FloodgateRatingResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloodgateRatingResult {
    pub player_name: String,
    pub rating: f64,
    pub effective_games: f64,
    pub connected_to_anchor: bool,
    pub anchor_margin: f64,
    pub snapshot_percentile: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct RatingPolicy {
    pub medium_games: f64,
    pub high_games: f64,
    pub min_component_size: i32,
}

pub fn import_tournament_ranking(
    connection: &mut Connection,
    document: &TournamentRanking,
) -> Result<i64, MetadataError> {
    validate_ranking(document)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE ranking_snapshot SET active=0 WHERE site=?1 AND event=?2",
        params![document.site, document.event],
    )?;
    transaction.execute(
        "INSERT INTO ranking_snapshot(
             site,event,source_url,retrieved_at,source_sha256,provisional,policy_version,active
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,1)",
        params![
            document.site,
            document.event,
            document.source_url,
            document.retrieved_at,
            document.source_sha256,
            i32::from(document.provisional),
            POLICY_VERSION,
        ],
    )?;
    let snapshot_id = transaction.last_insert_rowid();
    for result in &document.results {
        transaction.execute(
            "INSERT OR IGNORE INTO participant(event,official_name,normalized_name)
             VALUES(?1,?2,?3)",
            params![
                document.event,
                result.official_name,
                normalize_name(&result.official_name)
            ],
        )?;
        let participant_id: i64 = transaction.query_row(
            "SELECT id FROM participant WHERE event=?1 AND official_name=?2",
            params![document.event, result.official_name],
            |row| row.get(0),
        )?;
        let percentile =
            1.0 - f64::from(result.rank - 1) / f64::from((result.participants - 1).max(1));
        transaction.execute(
            "INSERT INTO ranking_result(
                 snapshot_id,participant_id,stage,stage_tier,rank,participants,rank_percentile
             ) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                snapshot_id,
                participant_id,
                result.stage,
                result.stage_tier,
                result.rank,
                result.participants,
                percentile,
            ],
        )?;
    }
    transaction.commit()?;
    Ok(snapshot_id)
}

pub fn import_floodgate_rating(
    connection: &mut Connection,
    document: &FloodgateRating,
    policy: RatingPolicy,
) -> Result<i64, MetadataError> {
    validate_rating(document, policy)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "UPDATE rating_snapshot SET active=0 WHERE year=?1",
        params![document.year],
    )?;
    transaction.execute(
        "INSERT INTO rating_snapshot(
             year,snapshot_time,source_url,source_sha256,anchor_era,rated_player_count,
             component_size,anchor_connected_rate,active
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1)",
        params![
            document.year,
            document.snapshot_time,
            document.source_url,
            document.source_sha256,
            document.anchor_era,
            document.rated_player_count,
            document.component_size,
            document.anchor_connected_rate,
        ],
    )?;
    let snapshot_id = transaction.last_insert_rowid();
    for result in &document.results {
        let reliability = if document.component_size < policy.min_component_size {
            1
        } else if result.connected_to_anchor && result.effective_games >= policy.high_games {
            3
        } else if result.connected_to_anchor && result.effective_games >= policy.medium_games {
            2
        } else {
            1
        };
        transaction.execute(
            "INSERT INTO rating_result(
                 snapshot_id,player_name,rating,effective_games,connected_to_anchor,
                 reliability_bucket,anchor_margin,snapshot_percentile
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                snapshot_id,
                normalize_name(&result.player_name),
                result.rating,
                result.effective_games,
                i32::from(result.connected_to_anchor),
                reliability,
                result.anchor_margin,
                result.snapshot_percentile,
            ],
        )?;
    }
    transaction.commit()?;
    Ok(snapshot_id)
}

pub fn add_participant_alias(
    connection: &mut Connection,
    event: &str,
    alias: &str,
    official_name: &str,
) -> Result<(), MetadataError> {
    let participant_id = connection
        .query_row(
            "SELECT id FROM participant WHERE event=?1 AND official_name=?2",
            params![event, official_name],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or(MetadataError::UnknownParticipant)?;
    let normalized_alias = normalize_name(alias);
    let existing: Option<i64> = connection
        .query_row(
            "SELECT participant_id FROM participant_alias WHERE event=?1 AND alias=?2",
            params![event, normalized_alias],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some_and(|existing| existing != participant_id) {
        return Err(MetadataError::AliasCollision {
            event: event.to_owned(),
            alias: normalized_alias,
        });
    }
    connection.execute(
        "INSERT INTO participant_alias(event,alias,participant_id,mapping_status)
         VALUES(?1,?2,?3,'manual') ON CONFLICT(event,alias) DO NOTHING",
        params![event, normalized_alias, participant_id],
    )?;
    Ok(())
}

pub fn import_participant_aliases(
    connection: &mut Connection,
    document: &ParticipantAliasDocument,
) -> Result<usize, MetadataError> {
    if document.event.trim().is_empty() {
        return Err(MetadataError::Invalid("alias event is required"));
    }
    for alias in &document.aliases {
        if alias.alias.trim().is_empty() || alias.official_name.trim().is_empty() {
            return Err(MetadataError::Invalid("alias names are required"));
        }
        add_participant_alias(
            connection,
            &document.event,
            &alias.alias,
            &alias.official_name,
        )?;
    }
    Ok(document.aliases.len())
}

pub fn unmatched_participants(
    connection: &Connection,
) -> Result<BTreeMap<String, i64>, MetadataError> {
    let rows: Vec<(i64, String, String, i32, String)> = connection
        .prepare(
            "SELECT DISTINCT game_id,site,event,year,player_name FROM (
                 SELECT lg.id AS game_id,rs.site,rs.event,rs.year,lg.black_name AS player_name
                 FROM logical_game lg
                 JOIN source_game sg ON sg.game_id=lg.id
                 JOIN raw_source rs ON rs.id=sg.source_id
                 UNION ALL
                 SELECT lg.id AS game_id,rs.site,rs.event,rs.year,lg.white_name AS player_name
                 FROM logical_game lg
                 JOIN source_game sg ON sg.game_id=lg.id
                 JOIN raw_source rs ON rs.id=sg.source_id
             ) ORDER BY game_id,site,event,year,player_name",
        )?
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut tournament_cache = HashMap::new();
    let mut rating_cache = HashMap::new();
    let mut normalized_name_cache = HashMap::new();
    let mut unmatched = BTreeMap::new();
    for (_game_id, site, event, year, player) in rows {
        let matched = if site == "floodgate" {
            rating_result(
                connection,
                &mut rating_cache,
                &mut normalized_name_cache,
                year,
                &player,
            )?
            .is_some()
        } else {
            tournament_result(
                connection,
                &mut tournament_cache,
                &mut normalized_name_cache,
                &event,
                &player,
            )?
            .is_some()
        };
        if !matched {
            *unmatched.entry(normalize_name(&player)).or_insert(0) += 1;
        }
    }
    Ok(unmatched)
}
pub fn recompute_candidate_priorities(
    connection: &mut Connection,
    reference_year: i32,
) -> Result<usize, MetadataError> {
    recompute_candidate_priorities_with_control(connection, reference_year, || false)
}

pub fn recompute_candidate_priorities_with_control<F>(
    connection: &mut Connection,
    reference_year: i32,
    mut should_stop: F,
) -> Result<usize, MetadataError>
where
    F: FnMut() -> bool,
{
    if reference_year <= 0 {
        return Err(MetadataError::Invalid("reference year must be positive"));
    }
    if should_stop() {
        return Err(MetadataError::Stopped);
    }
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS game_position_candidate_lookup_idx
         ON game_position(position_id,move,game_id,ply);
         CREATE INDEX IF NOT EXISTS source_game_game_lookup_idx
         ON source_game(game_id,source_id);",
    )?;
    let mut last_candidate_id = 0_i64;
    let mut tournament_cache = HashMap::new();
    let mut rating_cache = HashMap::new();
    let mut normalized_name_cache = HashMap::new();
    let (source_facts, event_names) = load_source_facts(connection)?;
    let (game_facts, player_names) = load_game_facts(connection)?;
    let total_rows: u64 =
        connection.query_row("SELECT COUNT(*) FROM candidate", [], |row| row.get(0))?;
    let mut processed_rows = 0_u64;
    let started = Instant::now();
    let mut last_progress = Instant::now();
    loop {
        if should_stop() {
            return Err(MetadataError::Stopped);
        }
        let page_started = Instant::now();
        let candidate_ids: Vec<i64> = connection
            .prepare("SELECT id FROM candidate WHERE id>?1 ORDER BY id LIMIT 1000")?
            .query_map([last_candidate_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        let Some(&page_last_id) = candidate_ids.last() else {
            break;
        };
        let rows: Vec<PriorityRow> = connection
            .prepare(
                "SELECT c.id,gp.ply,gp.game_id,sg.source_id
                 FROM candidate c
                 JOIN game_position gp ON c.position_id=gp.position_id AND c.move=gp.move
                 JOIN source_game sg ON sg.game_id=gp.game_id
                 WHERE c.id>?1 AND c.id<=?2
                 ORDER BY c.id,gp.game_id,gp.ply,sg.source_id",
            )?
            .query_map(params![last_candidate_id, page_last_id], |row| {
                Ok(PriorityRow {
                    candidate_id: row.get(0)?,
                    ply: row.get(1)?,
                    game_id: row.get(2)?,
                    source_id: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let query_elapsed = page_started.elapsed();
        let mut grouped: BTreeMap<i64, Vec<PriorityRow>> = BTreeMap::new();
        for row in rows {
            grouped.entry(row.candidate_id).or_default().push(row);
        }
        let mut updates = Vec::with_capacity(grouped.len());
        for (candidate_id, rows) in grouped {
            let unique_games: HashSet<i64> = rows.iter().map(|row| row.game_id).collect();
            let occurrences = unique_games.len() as i64;
            let recent_occurrences = rows
                .iter()
                .filter(|row| {
                    source_facts
                        .get(row.source_id as usize)
                        .and_then(|value| *value)
                        .and_then(|source| {
                            quality_decision(
                                reference_year,
                                &QualityFacts {
                                    event_year: source.year,
                                    strength_tier: 4,
                                },
                            )
                            .ok()
                        })
                        .is_some_and(|value| value.age_distance == 0)
                })
                .map(|row| row.game_id)
                .collect::<HashSet<_>>()
                .len() as i64;
            let mut provenances = Vec::with_capacity(rows.len());
            for row in rows {
                let source = source_facts
                    .get(row.source_id as usize)
                    .and_then(|value| *value)
                    .ok_or(MetadataError::Invalid("source facts missing"))?;
                let game = game_facts
                    .get(row.game_id as usize)
                    .and_then(|value| *value)
                    .ok_or(MetadataError::Invalid("game facts missing"))?;
                let (mover_id, opponent_id) = if row.ply % 2 == 0 {
                    (game.black_name_id, game.white_name_id)
                } else {
                    (game.white_name_id, game.black_name_id)
                };
                let mover = player_names
                    .get(mover_id)
                    .ok_or(MetadataError::Invalid("player name missing"))?;
                let opponent = player_names
                    .get(opponent_id)
                    .ok_or(MetadataError::Invalid("player name missing"))?;
                let event = event_names
                    .get(source.event_id)
                    .ok_or(MetadataError::Invalid("event name missing"))?;
                let mover_result = tournament_result(
                    connection,
                    &mut tournament_cache,
                    &mut normalized_name_cache,
                    event,
                    mover,
                )?;
                let opponent_result = tournament_result(
                    connection,
                    &mut tournament_cache,
                    &mut normalized_name_cache,
                    event,
                    opponent,
                )?;
                let rating = if source.site == SourceSite::Floodgate {
                    rating_result(
                        connection,
                        &mut rating_cache,
                        &mut normalized_name_cache,
                        source.year,
                        mover,
                    )?
                } else {
                    None
                };
                let mover_tier = if source.site == SourceSite::Floodgate {
                    rating.map_or(4, |value| {
                        floodgate_strength_tier(FloodgateStrength {
                            high_reliability: value.0 >= 3,
                            connected_to_anchor: value.3,
                            enough_effective_games: value.0 >= 3,
                            snapshot_percentile: value.2,
                        })
                    })
                } else {
                    mover_result.map_or(4, |value| official_strength_tier(value.strength))
                };
                let opponent_tier =
                    opponent_result.map_or(4, |value| official_strength_tier(value.strength));
                let quality = quality_decision(
                    reference_year,
                    &QualityFacts {
                        event_year: source.year,
                        strength_tier: mover_tier,
                    },
                )?;
                let facts = PriorityFacts {
                    year: source.year,
                    stage_tier: 4 - mover_tier,
                    mover_percentile: mover_result.map_or_else(
                        || rating.map_or(0.0, |value| value.2),
                        |value| value.percentile,
                    ),
                    opponent_stage_tier: 4 - opponent_tier,
                    opponent_percentile: opponent_result.map_or(0.0, |value| value.percentile),
                    rating_reliability: rating.map_or(0, |value| value.0),
                    anchor_margin: rating.map_or(0.0, |value| value.1),
                    rating_percentile: rating.map_or(0.0, |value| value.2),
                    recent_occurrences,
                    occurrences,
                    exact_time: source.exact_time,
                    ..PriorityFacts::default()
                };
                provenances.push(CandidateProvenance {
                    quality_band: quality.quality_band,
                    source_site: source.site,
                    priority_key: encode_priority(&priority_tuple(&facts)?)?,
                    game_id: row.game_id,
                    ply: row.ply,
                    source_id: row.source_id,
                });
            }
            let representative = provenances
                .into_iter()
                .min_by(|left, right| {
                    left.quality_band
                        .cmp(&right.quality_band)
                        .then_with(|| right.priority_key.cmp(&left.priority_key))
                        .then_with(|| left.source_id.cmp(&right.source_id))
                })
                .expect("candidate occurrence group is nonempty");
            updates.push((
                candidate_id,
                representative,
                recent_occurrences,
                occurrences,
            ));
        }
        let compute_elapsed = page_started.elapsed().saturating_sub(query_elapsed);
        let update_started = Instant::now();
        let transaction = connection.transaction()?;
        for (candidate_id, representative, recent_occurrences, occurrences) in updates {
            transaction.execute(
                "UPDATE candidate SET priority_key=?1,quality_band=?2,source_site=?3,
                    representative_game_id=?4,representative_ply=?5,source_id=?6,
                    recent_occurrences=?7,occurrences=?8,updated_at=strftime('%s','now')
                 WHERE id=?9",
                params![
                    representative.priority_key.as_slice(),
                    representative.quality_band,
                    representative.source_site as i32,
                    representative.game_id,
                    representative.ply,
                    representative.source_id,
                    recent_occurrences,
                    occurrences,
                    candidate_id,
                ],
            )?;
        }
        transaction.commit()?;
        let update_elapsed = update_started.elapsed();
        processed_rows += candidate_ids.len() as u64;
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        let rows_per_second = processed_rows as f64 / elapsed;
        let eta_seconds =
            total_rows.saturating_sub(processed_rows) as f64 / rows_per_second.max(f64::EPSILON);
        if processed_rows == total_rows
            || processed_rows % 100_000 == 0
            || last_progress.elapsed().as_secs() >= 30
        {
            eprintln!(
                "[corpus] phase=metadata event=priority_progress rows={} rows_total={} percent={:.2} rows_per_second={:.0} eta_seconds={:.1} query_ms={} compute_ms={} update_ms={}",
                processed_rows,
                total_rows,
                100.0 * processed_rows as f64 / total_rows.max(1) as f64,
                rows_per_second,
                eta_seconds,
                query_elapsed.as_millis(),
                compute_elapsed.as_millis(),
                update_elapsed.as_millis()
            );
            last_progress = Instant::now();
        }
        last_candidate_id = page_last_id;
    }
    connection.execute_batch(
        "DROP INDEX IF EXISTS game_position_candidate_lookup_idx;
         DROP INDEX IF EXISTS source_game_game_lookup_idx;",
    )?;
    Ok(connection.query_row("SELECT COUNT(*) FROM candidate", [], |row| row.get(0))?)
}

fn intern_value(ids: &mut HashMap<String, usize>, values: &mut Vec<String>, value: &str) -> usize {
    if let Some(id) = ids.get(value) {
        return *id;
    }
    let id = values.len();
    values.push(value.to_owned());
    ids.insert(value.to_owned(), id);
    id
}

fn load_source_facts(
    connection: &Connection,
) -> Result<(Vec<Option<SourceFacts>>, Vec<String>), rusqlite::Error> {
    let max_id: i64 =
        connection.query_row("SELECT COALESCE(MAX(id),0) FROM raw_source", [], |row| {
            row.get(0)
        })?;
    let mut facts = vec![None; max_id as usize + 1];
    let mut event_ids = HashMap::new();
    let mut events = Vec::new();
    let mut statement =
        connection.prepare("SELECT id,site,event,year,retrieved_at FROM raw_source ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i32>(3)?,
            row.get::<_, f64>(4)?,
        ))
    })?;
    for row in rows {
        let (id, site, event, year, exact_time) = row?;
        facts[id as usize] = Some(SourceFacts {
            site: SourceSite::from_name(&site),
            event_id: intern_value(&mut event_ids, &mut events, &event),
            year,
            exact_time,
        });
    }
    Ok((facts, events))
}

fn load_game_facts(
    connection: &Connection,
) -> Result<(Vec<Option<GameFacts>>, Vec<String>), rusqlite::Error> {
    let max_id: i64 =
        connection.query_row("SELECT COALESCE(MAX(id),0) FROM logical_game", [], |row| {
            row.get(0)
        })?;
    let mut facts = vec![None; max_id as usize + 1];
    let mut player_ids = HashMap::new();
    let mut players = Vec::new();
    let mut statement =
        connection.prepare("SELECT id,black_name,white_name FROM logical_game ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, black, white) = row?;
        facts[id as usize] = Some(GameFacts {
            black_name_id: intern_value(&mut player_ids, &mut players, &black),
            white_name_id: intern_value(&mut player_ids, &mut players, &white),
        });
    }
    Ok((facts, players))
}
#[derive(Debug)]
struct PriorityRow {
    candidate_id: i64,
    ply: i64,
    game_id: i64,
    source_id: i64,
}

#[derive(Clone, Copy, Debug)]
struct SourceFacts {
    site: SourceSite,
    event_id: usize,
    year: i32,
    exact_time: f64,
}

#[derive(Clone, Copy, Debug)]
struct GameFacts {
    black_name_id: usize,
    white_name_id: usize,
}

#[derive(Debug)]
struct CandidateProvenance {
    quality_band: i32,
    source_site: SourceSite,
    priority_key: [u8; 88],
    game_id: i64,
    ply: i64,
    source_id: i64,
}

#[derive(Clone, Copy, Debug)]
struct TournamentMatch {
    strength: OfficialStrength,
    percentile: f64,
}

type NormalizedNameCache = HashMap<String, String>;
type TournamentCache = HashMap<String, HashMap<String, Option<TournamentMatch>>>;
type RatingValue = (i32, f64, f64, bool);
type RatingCache = HashMap<i32, HashMap<String, Option<RatingValue>>>;

fn normalized_name_cached<'a>(cache: &'a mut NormalizedNameCache, player: &str) -> &'a str {
    cache
        .entry(player.to_owned())
        .or_insert_with(|| normalize_name(player))
        .as_str()
}

fn tournament_result(
    connection: &Connection,
    cache: &mut TournamentCache,
    normalized_names: &mut NormalizedNameCache,
    event: &str,
    player: &str,
) -> Result<Option<TournamentMatch>, rusqlite::Error> {
    let normalized = normalized_name_cached(normalized_names, player);
    if let Some(value) = cache.get(event).and_then(|players| players.get(normalized)) {
        return Ok(*value);
    }
    let value = connection
        .query_row(
            "SELECT rr.stage_tier,rr.rank,rr.participants,rr.rank_percentile,
                    (SELECT MIN(x.stage_tier) FROM ranking_result x WHERE x.snapshot_id=rr.snapshot_id),
                    (SELECT MAX(x.stage_tier) FROM ranking_result x WHERE x.snapshot_id=rr.snapshot_id)
             FROM ranking_snapshot rs
             JOIN ranking_result rr ON rr.snapshot_id=rs.id
             JOIN participant p ON p.id=rr.participant_id
             LEFT JOIN participant_alias pa ON pa.participant_id=p.id AND pa.event=p.event
             WHERE rs.active=1 AND rs.event=?1
               AND (p.normalized_name=?2 OR pa.alias=?2)
             ORDER BY rs.provisional ASC,rs.retrieved_at DESC LIMIT 1",
            params![event, normalized],
            |row| {
                let stage_tier: i32 = row.get(0)?;
                let rank = row.get(1)?;
                let participants = row.get(2)?;
                let percentile = row.get(3)?;
                let min_stage: i32 = row.get(4)?;
                let max_stage: i32 = row.get(5)?;
                let strength = if min_stage == max_stage {
                    OfficialStrength::SingleStage { rank, participants }
                } else if stage_tier == max_stage {
                    OfficialStrength::TopStage { rank, participants }
                } else if stage_tier == max_stage - 1 {
                    OfficialStrength::OneStageBelow { rank, participants }
                } else {
                    OfficialStrength::TwoOrMoreStagesBelow
                };
                Ok(TournamentMatch { strength, percentile })
            },
        )
        .optional()?;
    cache
        .entry(event.to_owned())
        .or_default()
        .insert(normalized.to_owned(), value);
    Ok(value)
}

fn rating_result(
    connection: &Connection,
    cache: &mut RatingCache,
    normalized_names: &mut NormalizedNameCache,
    year: i32,
    player: &str,
) -> Result<Option<RatingValue>, rusqlite::Error> {
    let normalized = normalized_name_cached(normalized_names, player);
    if let Some(value) = cache.get(&year).and_then(|players| players.get(normalized)) {
        return Ok(*value);
    }
    let value = connection
        .query_row(
            "SELECT rr.reliability_bucket,rr.anchor_margin,rr.snapshot_percentile,
                    rr.connected_to_anchor
             FROM rating_snapshot rs JOIN rating_result rr ON rr.snapshot_id=rs.id
             WHERE rs.active=1 AND rs.year=?1 AND rr.player_name=?2
             ORDER BY rs.snapshot_time DESC LIMIT 1",
            params![year, normalized],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    cache
        .entry(year)
        .or_default()
        .insert(normalized.to_owned(), value);
    Ok(value)
}
fn normalize_name(value: &str) -> String {
    value.nfkc().collect::<String>().trim().to_owned()
}

fn validate_ranking(document: &TournamentRanking) -> Result<(), MetadataError> {
    if document.site.trim().is_empty()
        || document.event.trim().is_empty()
        || document.source_url.trim().is_empty()
        || document.source_sha256.len() != 64
        || !document
            .source_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
        || !document.retrieved_at.is_finite()
        || document.results.iter().any(|result| {
            result.official_name.trim().is_empty()
                || result.stage.trim().is_empty()
                || result.stage_tier < 0
                || result.participants <= 0
                || result.rank <= 0
                || result.rank > result.participants
        })
    {
        return Err(MetadataError::Invalid("invalid tournament ranking"));
    }
    Ok(())
}

fn validate_rating(document: &FloodgateRating, policy: RatingPolicy) -> Result<(), MetadataError> {
    if document.year <= 0
        || !document.snapshot_time.is_finite()
        || document.source_url.trim().is_empty()
        || document.source_sha256.len() != 64
        || document.anchor_era.trim().is_empty()
        || document.rated_player_count < 0
        || document.component_size < 0
        || !(0.0..=1.0).contains(&document.anchor_connected_rate)
        || policy.medium_games < 0.0
        || policy.high_games < policy.medium_games
        || policy.min_component_size < 0
        || document.results.iter().any(|result| {
            result.player_name.trim().is_empty()
                || !result.rating.is_finite()
                || !result.effective_games.is_finite()
                || result.effective_games < 0.0
                || !result.anchor_margin.is_finite()
                || !(0.0..=1.0).contains(&result.snapshot_percentile)
        })
    {
        return Err(MetadataError::Invalid("invalid floodgate rating"));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("priority error: {0}")]
    Priority(#[from] PriorityError),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("metadata recomputation stopped")]
    Stopped,
    #[error("participant does not exist")]
    UnknownParticipant,
    #[error("alias collision for {event}/{alias}")]
    AliasCollision { event: String, alias: String },
}
