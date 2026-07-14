use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut waiting_for_stop = false;
    let mut position = String::new();
    let mut generate_all_legal_moves = false;
    let mut mode_switch_pending_clear = false;
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        if line.starts_with("position ") {
            position.clone_from(&line);
        } else if line == "usi" {
            writeln!(stdout, "id name fake-usi-engine").unwrap();
            writeln!(
                stdout,
                "option name USI_Hash type spin default 16 min 1 max 1024"
            )
            .unwrap();
            writeln!(
                stdout,
                "option name Threads type spin default 1 min 1 max 128"
            )
            .unwrap();
            writeln!(
                stdout,
                "option name MultiPV type spin default 1 min 1 max 128"
            )
            .unwrap();
            writeln!(
                stdout,
                "option name GenerateAllLegalMoves type check default false"
            )
            .unwrap();
            writeln!(stdout, "option name Clear Hash type button").unwrap();
            writeln!(stdout, "usiok").unwrap();
        } else if line == "setoption name GenerateAllLegalMoves value true" {
            if !generate_all_legal_moves {
                generate_all_legal_moves = true;
                mode_switch_pending_clear = true;
            }
        } else if line == "setoption name GenerateAllLegalMoves value false" {
            if generate_all_legal_moves {
                generate_all_legal_moves = false;
                mode_switch_pending_clear = true;
            }
        } else if line == "setoption name Clear Hash" {
            mode_switch_pending_clear = false;
        } else if line == "isready" {
            writeln!(stdout, "readyok").unwrap();
        } else if line == "go nodes 991" {
            writeln!(stdout, "bestmove").unwrap();
        } else if line == "go nodes 994" {
            writeln!(stdout, "bestmove resign").unwrap();
        } else if line == "go nodes 993" {
            writeln!(stdout, "bestmove win").unwrap();
        } else if line == "go nodes 992" {
            writeln!(stdout, "bestmove none").unwrap();
        } else if line == "go nodes 996" {
            std::process::exit(23);
        } else if line == "go nodes 997" {
            waiting_for_stop = false;
        } else if line == "go nodes 999" {
            waiting_for_stop = true;
        } else if line == "stop" && waiting_for_stop {
            waiting_for_stop = false;
            writeln!(stdout, "bestmove none").unwrap();
        } else if line.starts_with("go ") && line.contains("searchmoves 8g8f") {
            if generate_all_legal_moves && !mode_switch_pending_clear {
                writeln!(
                    stdout,
                    "info depth 11 multipv 1 score cp 77 nodes 100 pv 8g8f 3c3d"
                )
                .unwrap();
            }
            writeln!(stdout, "bestmove 8g8f ponder 3c3d").unwrap();
        } else if line.starts_with("go ") && line.contains("searchmoves 7g7f") {
            if generate_all_legal_moves && !mode_switch_pending_clear {
                writeln!(
                    stdout,
                    "info depth 15 multipv 1 score cp 135 lowerbound nodes 100 pv 7g7f"
                )
                .unwrap();
            }
            writeln!(stdout, "bestmove 7g7f ponder 3c3d").unwrap();
        } else if line.starts_with("go ") {
            if line == "go nodes 995" {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            if line == "go nodes 998" {
                std::thread::sleep(std::time::Duration::from_millis(180));
            }
            if generate_all_legal_moves || mode_switch_pending_clear {
                writeln!(stdout, "bestmove none").unwrap();
            } else if position == "position startpos" {
                writeln!(stdout, "info depth 10 multipv 1 score cp 25 pv 7g7f 3c3d").unwrap();
                writeln!(stdout, "info depth 9 multipv 2 score cp 10 pv 2g2f 8c8d").unwrap();
                writeln!(stdout, "bestmove 7g7f ponder 3c3d").unwrap();
            } else if position == "position startpos moves 2g2f" {
                writeln!(stdout, "info depth 10 multipv 1 score cp 25 pv 8c8d 2f2e").unwrap();
                writeln!(stdout, "info depth 9 multipv 2 score cp 10 pv 3c3d 2f2e").unwrap();
                writeln!(stdout, "bestmove 8c8d ponder 2f2e").unwrap();
            } else if position == "position startpos moves 7g7f" {
                writeln!(stdout, "info depth 10 multipv 1 score cp 25 pv 3c3d 2g2f").unwrap();
                writeln!(stdout, "info depth 9 multipv 2 score cp 10 pv 8c8d 2g2f").unwrap();
                writeln!(stdout, "bestmove 3c3d ponder 2g2f").unwrap();
            } else {
                writeln!(stdout, "info depth 10 multipv 1 score cp 25 pv 2g2f 8c8d").unwrap();
                writeln!(stdout, "info depth 9 multipv 2 score cp 10 pv 8g8f 3a3b").unwrap();
                writeln!(stdout, "bestmove 2g2f ponder 8c8d").unwrap();
            }
        } else if line == "quit" {
            break;
        }
        stdout.flush().unwrap();
    }
}
