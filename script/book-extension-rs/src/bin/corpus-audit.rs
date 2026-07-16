use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
    time::Instant,
};

use book_extension_runtime::{
    corpus_source::{ArchiveLimits, RecordFormat, VisitRecordsError, visit_records},
    csa::parse_csa,
    kif::parse_kif,
};
use clap::Parser;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    source: PathBuf,
    #[arg(long)]
    member_pattern: Option<String>,
    #[arg(long)]
    seven_zip: Option<PathBuf>,
    #[arg(long, default_value_t = 10_000)]
    progress_every: u64,
    #[arg(long)]
    error_log: Option<PathBuf>,
    #[arg(long)]
    game_log: Option<PathBuf>,
}

#[derive(Debug, Default, Serialize)]
struct AuditSummary {
    records: u64,
    accepted_games: u64,
    excluded_records: u64,
    moves: u64,
    reasons: BTreeMap<String, u64>,
    elapsed_seconds: f64,
    records_per_second: f64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(summary) => {
            println!(
                "{}",
                serde_json::to_string(&summary).expect("audit summary is serializable")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[corpus-audit] event=fatal detail={error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Args) -> Result<AuditSummary, String> {
    let started = Instant::now();
    let mut summary = AuditSummary::default();
    let mut error_log = args
        .error_log
        .as_ref()
        .map(|path| {
            File::create(path)
                .map(BufWriter::new)
                .map_err(|error| format!("failed to create {}: {error}", path.display()))
        })
        .transpose()?;
    let mut game_log = args
        .game_log
        .as_ref()
        .map(|path| {
            File::create(path)
                .map(BufWriter::new)
                .map_err(|error| format!("failed to create {}: {error}", path.display()))
        })
        .transpose()?;
    let result = visit_records(
        &args.source,
        args.member_pattern.as_deref(),
        ArchiveLimits::default(),
        args.seven_zip.as_deref(),
        |record| {
            summary.records += 1;
            let parsed = match record.format {
                RecordFormat::Csa => {
                    parse_csa(&record.bytes, &record.relative_path).map(|game| vec![game])
                }
                RecordFormat::Kif => parse_kif(&record.bytes, &record.relative_path),
            };
            match parsed {
                Ok(games) => {
                    summary.accepted_games += games.len() as u64;
                    summary.moves += games
                        .iter()
                        .map(|game| game.moves.len() as u64)
                        .sum::<u64>();
                    if let Some(writer) = game_log.as_mut() {
                        for game in &games {
                            write_game_log(&mut *writer, game)?;
                        }
                    }
                }
                Err(error) => {
                    summary.excluded_records += 1;
                    *summary
                        .reasons
                        .entry(format!("{:?}", error.kind))
                        .or_default() += 1;
                    if let Some(writer) = error_log.as_mut() {
                        serde_json::to_writer(
                            &mut *writer,
                            &json!({
                                "kind": format!("{:?}", error.kind),
                                "source_path": error.source_path,
                                "line": error.line,
                                "previous_sfen": error.previous_sfen,
                                "move": error.move_text,
                                "detail": error.detail,
                            }),
                        )
                        .map_err(|error| format!("failed to write error log: {error}"))?;
                        writer
                            .write_all(b"\n")
                            .map_err(|error| format!("failed to write error log: {error}"))?;
                    }
                }
            }
            if args.progress_every != 0 && summary.records.is_multiple_of(args.progress_every) {
                let elapsed = started.elapsed().as_secs_f64();
                let rate = summary.records as f64 / elapsed.max(f64::EPSILON);
                eprintln!(
                    "[corpus-audit] event=progress records={} accepted_games={} excluded_records={} moves={} elapsed_seconds={elapsed:.3} records_per_second={rate:.3}",
                    summary.records,
                    summary.accepted_games,
                    summary.excluded_records,
                    summary.moves
                );
            }
            Ok::<_, String>(())
        },
    );
    match result {
        Ok(_) => {}
        Err(VisitRecordsError::Source(error)) => return Err(error.to_string()),
        Err(VisitRecordsError::Visitor(error)) => return Err(error),
    }
    if let Some(writer) = error_log.as_mut() {
        writer
            .flush()
            .map_err(|error| format!("failed to flush error log: {error}"))?;
    }
    if let Some(writer) = game_log.as_mut() {
        writer
            .flush()
            .map_err(|error| format!("failed to flush game log: {error}"))?;
    }
    summary.elapsed_seconds = started.elapsed().as_secs_f64();
    summary.records_per_second = summary.records as f64 / summary.elapsed_seconds.max(f64::EPSILON);
    Ok(summary)
}

fn write_game_log(
    writer: &mut impl Write,
    game: &book_extension_runtime::record::NormalizedGame,
) -> Result<(), String> {
    let mut moves_digest = Sha256::new();
    for move_usi in &game.moves {
        update_string(&mut moves_digest, move_usi);
    }
    let mut positions_digest = Sha256::new();
    for position in &game.positions {
        update_string(&mut positions_digest, &position.sfen);
        update_string(&mut positions_digest, &position.move_usi);
        positions_digest.update((position.ply as u64).to_be_bytes());
    }
    serde_json::to_writer(
        &mut *writer,
        &json!({
            "source_path": game.source_path,
            "sha256": game.sha256,
            "players": game.players,
            "move_count": game.moves.len(),
            "moves_sha256": format!("{:x}", moves_digest.finalize()),
            "position_count": game.positions.len(),
            "positions_sha256": format!("{:x}", positions_digest.finalize()),
            "endgame": game.endgame,
        }),
    )
    .map_err(|error| format!("failed to write game log: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("failed to write game log: {error}"))
}

fn update_string(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}
