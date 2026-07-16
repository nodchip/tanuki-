use std::{
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use book_extension_runtime::corpus_builder::{BuildOptions, build_corpus};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Build {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        input_book: Option<PathBuf>,
        #[arg(long)]
        seven_zip: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let args = Args::parse();
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = Arc::clone(&stop);
    if let Err(error) = ctrlc::set_handler(move || {
        signal_stop.store(true, Ordering::Relaxed);
    }) {
        eprintln!("[corpus] phase=startup event=fatal detail={error}");
        return ExitCode::from(2);
    }
    let options = match args.command {
        Command::Build {
            profile,
            state_dir,
            input_book,
            seven_zip,
        } => BuildOptions {
            profile_path: resolve_profile(profile),
            state_dir,
            input_book,
            seven_zip,
            stop,
        },
    };
    match build_corpus(&options) {
        Ok(summary) => {
            println!(
                "{}",
                serde_json::to_string(&summary).expect("summary is serializable")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[corpus] phase=build event=fatal detail={error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn resolve_profile(profile: PathBuf) -> PathBuf {
    if profile.is_file() {
        return profile;
    }
    let Some(name) = profile.to_str() else {
        return profile;
    };
    match name {
        "pilot" | "production" => PathBuf::from(format!("config/corpus-{name}.json")),
        _ => profile,
    }
}
