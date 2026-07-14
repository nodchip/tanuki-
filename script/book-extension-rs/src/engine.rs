use std::{
    collections::{BTreeMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::{
    search::SearchResult,
    usi::{PositionRoot, UsiParseError, build_position_command, parse_info_line},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineOptions {
    pub hash_mb: usize,
    pub threads: usize,
    pub multipv: usize,
    pub extra: Vec<(String, Option<String>)>,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("USI engine I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("USI parse failed: {0}")]
    Parse(#[from] UsiParseError),
    #[error("USI engine closed its output")]
    Eof,
    #[error("malformed USI bestmove line: {0}")]
    MalformedBestmove(String),
    #[error("engine returned no evaluated PV for searchmove {0}")]
    NoEvaluatedPv(String),
    #[error("USI engine did not exit after quit")]
    QuitTimeout,
    #[error("USI search timed out after {0:.3} seconds")]
    SearchTimeout(f64),
    #[error("USI stdin lock is poisoned")]
    Poisoned,
}

#[derive(Clone)]
pub struct StopHandle {
    stdin: Arc<Mutex<ChildStdin>>,
    searching: Arc<AtomicBool>,
    child: Arc<Mutex<Child>>,
}

impl StopHandle {
    pub fn stop(&self) -> Result<bool, EngineError> {
        if !self.searching.load(Ordering::Acquire) {
            return Ok(false);
        }
        send_locked(&self.stdin, "stop")?;
        Ok(true)
    }
    pub fn is_searching(&self) -> bool {
        self.searching.load(Ordering::Acquire)
    }

    pub fn force_kill(&self) -> Result<(), EngineError> {
        self.child
            .lock()
            .map_err(|_| EngineError::Poisoned)?
            .kill()?;
        Ok(())
    }
}

pub struct UsiEngine {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: BufReader<ChildStdout>,
    searching: Arc<AtomicBool>,
    option_names: HashSet<String>,
    generate_all_legal_moves: bool,
}

impl UsiEngine {
    pub fn start(path: &Path, options: EngineOptions) -> Result<Self, EngineError> {
        let mut child = Command::new(path)
            .current_dir(path.parent().unwrap_or_else(|| Path::new(".")))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or(EngineError::Eof)?));
        let stdout = BufReader::new(child.stdout.take().ok_or(EngineError::Eof)?);
        let child = Arc::new(Mutex::new(child));
        let mut engine = Self {
            child,
            stdin,
            stdout,
            searching: Arc::new(AtomicBool::new(false)),
            option_names: HashSet::new(),
            generate_all_legal_moves: false,
        };
        engine.send("usi")?;
        engine.wait_for("usiok")?;
        let hash_name = if engine.option_names.contains("USI_Hash") {
            "USI_Hash"
        } else {
            "Hash"
        };
        engine.set_option(hash_name, Some(&options.hash_mb.to_string()))?;
        engine.set_option("Threads", Some(&options.threads.to_string()))?;
        engine.set_option("MultiPV", Some(&options.multipv.to_string()))?;
        for (name, value) in &options.extra {
            engine.set_option(name, value.as_deref())?;
            if name.eq_ignore_ascii_case("GenerateAllLegalMoves") {
                engine.generate_all_legal_moves = value
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("true"));
            }
        }
        engine.send("isready")?;
        engine.wait_for("readyok")?;
        engine.send("usinewgame")?;
        Ok(engine)
    }

    pub fn restart(&mut self, path: &Path, options: EngineOptions) -> Result<(), EngineError> {
        let replacement = Self::start(path, options)?;
        let replacement_child = Arc::try_unwrap(replacement.child)
            .map_err(|_| EngineError::Poisoned)?
            .into_inner()
            .map_err(|_| EngineError::Poisoned)?;
        let replacement_stdin = Arc::try_unwrap(replacement.stdin)
            .map_err(|_| EngineError::Poisoned)?
            .into_inner()
            .map_err(|_| EngineError::Poisoned)?;
        *self.child.lock().map_err(|_| EngineError::Poisoned)? = replacement_child;
        *self.stdin.lock().map_err(|_| EngineError::Poisoned)? = replacement_stdin;
        self.stdout = replacement.stdout;
        self.option_names = replacement.option_names;
        self.generate_all_legal_moves = replacement.generate_all_legal_moves;
        self.searching.store(false, Ordering::Release);
        Ok(())
    }
    pub fn stop_handle(&self) -> StopHandle {
        StopHandle {
            stdin: Arc::clone(&self.stdin),
            searching: Arc::clone(&self.searching),
            child: Arc::clone(&self.child),
        }
    }

    pub fn search(
        &mut self,
        root: &PositionRoot,
        moves: &[&str],
        nodes: u64,
    ) -> Result<Vec<SearchResult>, EngineError> {
        self.ensure_generate_all_legal_moves(false)?;
        self.searching.store(true, Ordering::Release);
        let result = self.search_inner(root, moves, nodes);
        self.searching.store(false, Ordering::Release);
        result
    }

    fn search_inner(
        &mut self,
        root: &PositionRoot,
        moves: &[&str],
        nodes: u64,
    ) -> Result<Vec<SearchResult>, EngineError> {
        self.send(&build_position_command(root, moves))?;
        self.send(&format!("go nodes {nodes}"))?;
        let mut latest = BTreeMap::new();
        loop {
            let line = self.read_line()?;
            if line.starts_with("bestmove ") {
                break;
            }
            if line == "bestmove" {
                return Err(EngineError::MalformedBestmove(line));
            }
            if let Some((multipv, result)) = parse_info_line(&line, false)? {
                latest.insert(multipv, result);
            }
        }
        Ok(latest.into_values().collect())
    }

    pub fn search_move(
        &mut self,
        root: &PositionRoot,
        moves: &[&str],
        nodes: u64,
        move_usi: &str,
    ) -> Result<SearchResult, EngineError> {
        self.ensure_generate_all_legal_moves(true)?;
        self.searching.store(true, Ordering::Release);
        let result = self.search_move_inner(root, moves, nodes, move_usi);
        self.searching.store(false, Ordering::Release);
        result
    }

    fn search_move_inner(
        &mut self,
        root: &PositionRoot,
        moves: &[&str],
        nodes: u64,
        move_usi: &str,
    ) -> Result<SearchResult, EngineError> {
        self.send(&build_position_command(root, moves))?;
        self.send(&format!("go nodes {nodes} searchmoves {move_usi}"))?;
        let mut latest: Option<SearchResult> = None;
        loop {
            let line = self.read_line()?;
            if line.starts_with("bestmove ") {
                let tokens: Vec<&str> = line.split_whitespace().collect();
                if let Some(result) = latest.as_mut()
                    && result.response.eq_ignore_ascii_case("none")
                    && let Some(index) = tokens.iter().position(|token| *token == "ponder")
                    && let Some(response) = tokens.get(index + 1)
                {
                    result.response = (*response).to_owned();
                }
                break;
            }
            if let Some((_, result)) = parse_info_line(&line, true)?
                && result.move_usi == move_usi
            {
                latest = Some(result);
            }
        }
        latest.ok_or_else(|| EngineError::NoEvaluatedPv(move_usi.to_owned()))
    }

    pub fn close(&mut self) -> Result<(), EngineError> {
        if self
            .child
            .lock()
            .map_err(|_| EngineError::Poisoned)?
            .try_wait()?
            .is_some()
        {
            return Ok(());
        }
        self.send("quit")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self
                .child
                .lock()
                .map_err(|_| EngineError::Poisoned)?
                .try_wait()?
                .is_some()
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.child
                    .lock()
                    .map_err(|_| EngineError::Poisoned)?
                    .kill()?;
                return Err(EngineError::QuitTimeout);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn set_option(&self, name: &str, value: Option<&str>) -> Result<(), EngineError> {
        match value {
            Some(value) => self.send(&format!("setoption name {name} value {value}")),
            None => self.send(&format!("setoption name {name}")),
        }
    }

    fn ensure_generate_all_legal_moves(&mut self, desired: bool) -> Result<(), EngineError> {
        if self.generate_all_legal_moves == desired {
            return Ok(());
        }
        self.set_option(
            "GenerateAllLegalMoves",
            Some(if desired { "true" } else { "false" }),
        )?;
        self.set_option("Clear Hash", None)?;
        self.generate_all_legal_moves = desired;
        Ok(())
    }

    fn wait_for(&mut self, expected: &str) -> Result<(), EngineError> {
        loop {
            if self.read_line()? == expected {
                return Ok(());
            }
        }
    }

    fn read_line(&mut self) -> Result<String, EngineError> {
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            return Err(EngineError::Eof);
        }
        let line = line.trim().to_owned();
        if let Some(rest) = line.strip_prefix("option name ")
            && let Some((name, _)) = rest.split_once(" type ")
        {
            self.option_names.insert(name.to_owned());
        }
        Ok(line)
    }

    fn send(&self, command: &str) -> Result<(), EngineError> {
        send_locked(&self.stdin, command)
    }
}

fn send_locked(stdin: &Arc<Mutex<ChildStdin>>, command: &str) -> Result<(), EngineError> {
    let mut stdin = stdin.lock().map_err(|_| EngineError::Poisoned)?;
    writeln!(stdin, "{command}")?;
    stdin.flush()?;
    Ok(())
}
