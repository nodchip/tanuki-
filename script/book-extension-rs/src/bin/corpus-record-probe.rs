use std::{fs, path::PathBuf, process::ExitCode};

use book_extension_runtime::{csa::parse_csa, kif::parse_kif, record::RecordError};
use clap::{Parser, ValueEnum};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RecordFormat {
    Csa,
    Kif,
}

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, value_enum)]
    format: Option<RecordFormat>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string(&value).expect("JSON value is serializable")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!(
                "{}",
                serde_json::to_string(&error_json(&error)).expect("JSON value is serializable")
            );
            ExitCode::from(2)
        }
    }
}

fn run(args: &Args) -> Result<Value, RecordError> {
    let bytes = fs::read(&args.input).map_err(|error| {
        RecordError::new(
            &args.input.to_string_lossy(),
            book_extension_runtime::record::RecordErrorKind::Decode,
            None,
            format!("failed to read record: {error}"),
        )
    })?;
    let source = args.input.to_string_lossy();
    let format = args.format.unwrap_or_else(|| {
        if args
            .input
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("csa"))
        {
            RecordFormat::Csa
        } else {
            RecordFormat::Kif
        }
    });
    let games = match format {
        RecordFormat::Csa => vec![parse_csa(&bytes, &source)?],
        RecordFormat::Kif => parse_kif(&bytes, &source)?,
    };
    let games: Vec<_> = games
        .iter()
        .map(|game| {
            let positions: Vec<_> = game
                .positions
                .iter()
                .map(|position| {
                    json!({
                        "sfen": position.sfen,
                        "move": position.move_usi,
                        "ply": position.ply,
                    })
                })
                .collect();
            json!({
                "source_path": game.source_path,
                "sha256": game.sha256,
                "players": game.players,
                "moves": game.moves,
                "positions": positions,
                "endgame": game.endgame,
            })
        })
        .collect();
    Ok(json!({"status": "accepted", "games": games}))
}

fn error_json(error: &RecordError) -> Value {
    json!({
        "status": "excluded",
        "error": {
            "kind": format!("{:?}", error.kind),
            "line": error.line,
            "previous_sfen": error.previous_sfen,
            "move": error.move_text,
            "detail": error.detail,
            "source_path": error.source_path,
        }
    })
}
