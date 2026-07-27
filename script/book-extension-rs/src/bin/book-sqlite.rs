use std::{path::PathBuf, process::ExitCode};

use book_extension_runtime::sqlite_book::{SqliteOpeningBook, export_yaneuraou_atomic};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "book-sqlite")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Import {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = false)]
        ignore_ply: bool,
        #[arg(long, default_value_t = false)]
        allow_unsorted: bool,
    },
    Export {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 0)]
        backup_count: usize,
    },
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        Command::Import {
            database,
            input,
            ignore_ply,
            allow_unsorted,
        } => {
            let mut book = SqliteOpeningBook::open(&database, ignore_ply)?;
            let report = if allow_unsorted {
                book.import_yaneuraou_compatible(&input)?
            } else {
                book.import_yaneuraou(&input)?
            };
            eprintln!(
                "[book_import] source={} sha256={} already_imported={} positions={} moves={}",
                input.display(),
                report.input_sha256,
                report.already_imported,
                report.inserted_positions,
                report.inserted_moves
            );
        }
        Command::Export {
            database,
            output,
            backup_count,
        } => {
            let report = export_yaneuraou_atomic(&database, &output, backup_count)?;
            eprintln!(
                "[book_export] database={} output={} positions={} moves={}",
                database.display(),
                output.display(),
                report.positions,
                report.entries
            );
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("book-sqlite: {error}");
            ExitCode::from(1)
        }
    }
}
