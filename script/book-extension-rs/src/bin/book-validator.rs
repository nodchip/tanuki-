use std::{env, path::Path, process::ExitCode, time::Instant};

use book_extension_runtime::validation::validate_file;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("usage: book-validator <book-path>");
        return ExitCode::from(64);
    };
    let started = Instant::now();
    match validate_file(Path::new(&path)) {
        Ok(report) => {
            println!(
                "validate_sec={:.6} positions={} entries={} issues={}",
                started.elapsed().as_secs_f64(),
                report.positions,
                report.entries,
                report.issues.len()
            );
            for issue in &report.issues {
                println!(
                    "kind={} sfen={} move={} detail={}",
                    issue.kind.as_str(),
                    issue.sfen,
                    issue.move_usi.as_deref().unwrap_or("none"),
                    issue.detail
                );
            }
            if report.valid() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
