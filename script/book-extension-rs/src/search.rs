use std::collections::HashSet;

use shogi_legality_lite::{all_legal_moves_partial, is_legal_partial};
use thiserror::Error;

use crate::{
    book::{BookEntry, BookPosition, OpeningBook},
    python_random::PythonRandom,
    validation::{legal_distinct_entry_count, parse_move, parse_position},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub move_usi: String,
    pub response: String,
    pub eval_cp: i32,
    pub depth: i32,
}

impl SearchResult {
    pub fn new(
        move_usi: impl Into<String>,
        response: impl Into<String>,
        eval_cp: i32,
        depth: i32,
    ) -> Self {
        Self {
            move_usi: move_usi.into(),
            response: response.into(),
            eval_cp,
            depth,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathStep {
    pub sfen: String,
    pub move_usi: String,
}

impl PathStep {
    pub fn new(sfen: impl Into<String>, move_usi: impl Into<String>) -> Self {
        Self {
            sfen: sfen.into(),
            move_usi: move_usi.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeafPath {
    pub steps: Vec<PathStep>,
    pub leaf_sfen: String,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SearchError {
    #[error("eval_scale must be positive")]
    InvalidEvalScale,
    #[error("path position is missing: {0}")]
    MissingPosition(String),
    #[error("path move is missing: {sfen} {move_usi}")]
    MissingMove { sfen: String, move_usi: String },
    #[error("invalid SFEN: {0}")]
    InvalidSfen(String),
    #[error("illegal move {move_usi} in {sfen}")]
    IllegalMove { sfen: String, move_usi: String },
    #[error("book lookup failed: {0}")]
    Book(String),
}

pub trait SearchBook {
    fn search_position(&self, sfen: &str) -> Result<Option<BookPosition>, SearchError>;
    fn search_position_key(&self, sfen: &str) -> String;
}

impl SearchBook for OpeningBook {
    fn search_position(&self, sfen: &str) -> Result<Option<BookPosition>, SearchError> {
        Ok(self.position(sfen).cloned())
    }

    fn search_position_key(&self, sfen: &str) -> String {
        self.position_key(sfen)
    }
}

pub fn score_to_winrate(eval_cp: i32, eval_scale: f64) -> Result<f64, SearchError> {
    if eval_scale <= 0.0 {
        return Err(SearchError::InvalidEvalScale);
    }
    let x = (f64::from(eval_cp) / eval_scale).clamp(-60.0, 60.0);
    Ok(1.0 / (1.0 + (-x).exp()))
}

pub fn calculate_ucb(
    eval_cp: i32,
    child_visits: u64,
    parent_visits: u64,
    c_puct: f64,
    eval_scale: f64,
) -> Result<f64, SearchError> {
    let winrate = score_to_winrate(eval_cp, eval_scale)?;
    let exploration =
        c_puct * (((parent_visits + 1) as f64).ln() / ((child_visits + 1) as f64)).sqrt();
    Ok(winrate + exploration)
}

pub fn merge_search_results(
    position: &mut BookPosition,
    results: &[SearchResult],
    selected_move: Option<&str>,
) {
    let mut next_order = position
        .entries
        .iter()
        .map(|entry| entry.order_index)
        .max()
        .map_or(0, |order| order + 1);
    for result in results {
        if let Some(entry) = position.find_entry_mut(&result.move_usi) {
            if is_no_response(&entry.response) && !is_no_response(&result.response) {
                entry.response.clone_from(&result.response);
            }
        } else {
            position.entries.push(BookEntry::new(
                &result.move_usi,
                &result.response,
                result.eval_cp,
                result.depth,
                0,
                next_order,
            ));
            next_order += 1;
        }
    }
    if let Some(selected_move) = selected_move
        && let Some(selected) = position.find_entry_mut(selected_move)
    {
        selected.visits += 1;
    }
}

pub fn propagate_minimax(book: &mut OpeningBook, path: &LeafPath) -> Result<(), SearchError> {
    let mut current_value = node_value(book, &path.leaf_sfen);
    for step in path.steps.iter().rev() {
        let position = book
            .position_mut(&step.sfen)
            .ok_or_else(|| SearchError::MissingPosition(step.sfen.clone()))?;
        let entry =
            position
                .find_entry_mut(&step.move_usi)
                .ok_or_else(|| SearchError::MissingMove {
                    sfen: step.sfen.clone(),
                    move_usi: step.move_usi.clone(),
                })?;
        entry.eval_cp = -current_value;
        current_value = node_value(book, &step.sfen);
    }
    Ok(())
}

pub fn increment_path_visits(book: &mut OpeningBook, path: &LeafPath) -> Result<(), SearchError> {
    for step in &path.steps {
        let position = book
            .position_mut(&step.sfen)
            .ok_or_else(|| SearchError::MissingPosition(step.sfen.clone()))?;
        let entry =
            position
                .find_entry_mut(&step.move_usi)
                .ok_or_else(|| SearchError::MissingMove {
                    sfen: step.sfen.clone(),
                    move_usi: step.move_usi.clone(),
                })?;
        entry.visits += 1;
    }
    Ok(())
}

pub fn node_value(book: &OpeningBook, sfen: &str) -> i32 {
    book.position(sfen)
        .and_then(|position| position.entries.iter().map(|entry| entry.eval_cp).max())
        .unwrap_or(0)
}

fn is_no_response(response: &str) -> bool {
    response.eq_ignore_ascii_case("none")
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PetaFilter {
    pub book_side: Option<&'static str>,
    pub root_best_eval: i32,
    pub eval_diff: Option<i32>,
    pub min_eval_cp: Option<i32>,
}

pub fn reserve_leaf_path<B: SearchBook + ?Sized>(
    book: &B,
    root_sfen: &str,
    multipv: usize,
    c_puct: f64,
    eval_scale: f64,
    inflight: &mut HashSet<String>,
    max_ply: Option<usize>,
) -> Result<Option<LeafPath>, SearchError> {
    reserve_leaf_path_with_filter(
        book, root_sfen, multipv, c_puct, eval_scale, inflight, max_ply, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn reserve_leaf_path_with_filter<B: SearchBook + ?Sized>(
    book: &B,
    root_sfen: &str,
    multipv: usize,
    c_puct: f64,
    eval_scale: f64,
    inflight: &mut HashSet<String>,
    max_ply: Option<usize>,
    filter: Option<&PetaFilter>,
) -> Result<Option<LeafPath>, SearchError> {
    reserve_leaf_path_with_filter_and_random(
        book, root_sfen, multipv, c_puct, eval_scale, inflight, max_ply, filter, None,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn reserve_leaf_path_with_filter_and_random<B: SearchBook + ?Sized>(
    book: &B,
    root_sfen: &str,
    multipv: usize,
    c_puct: f64,
    eval_scale: f64,
    inflight: &mut HashSet<String>,
    max_ply: Option<usize>,
    filter: Option<&PetaFilter>,
    random: Option<&mut PythonRandom>,
) -> Result<Option<LeafPath>, SearchError> {
    let root_key = book.search_position_key(root_sfen);
    let mut visiting = HashSet::new();
    let mut dead = HashSet::new();
    let path = select_available_leaf(
        book,
        &root_key,
        multipv,
        c_puct,
        eval_scale,
        inflight,
        max_ply,
        filter,
        random,
        0,
        &mut visiting,
        &mut dead,
    )?;
    if let Some(path) = &path {
        inflight.insert(path.leaf_sfen.clone());
    }
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
fn select_available_leaf<B: SearchBook + ?Sized>(
    book: &B,
    sfen: &str,
    multipv: usize,
    c_puct: f64,
    eval_scale: f64,
    inflight: &HashSet<String>,
    max_ply: Option<usize>,
    filter: Option<&PetaFilter>,
    mut random: Option<&mut PythonRandom>,
    depth: usize,
    visiting: &mut HashSet<String>,
    dead: &mut HashSet<String>,
) -> Result<Option<LeafPath>, SearchError> {
    if max_ply.is_some_and(|limit| depth >= limit) {
        return Ok((!inflight.contains(sfen)).then(|| LeafPath {
            steps: Vec::new(),
            leaf_sfen: sfen.to_owned(),
        }));
    }
    if visiting.contains(sfen) || dead.contains(sfen) {
        return Ok(None);
    }
    let Some(position) = book.search_position(sfen)? else {
        return Ok((!inflight.contains(sfen)).then(|| LeafPath {
            steps: Vec::new(),
            leaf_sfen: sfen.to_owned(),
        }));
    };
    if position.entries.is_empty() {
        return Ok((!inflight.contains(sfen)).then(|| LeafPath {
            steps: Vec::new(),
            leaf_sfen: sfen.to_owned(),
        }));
    }
    let parsed = parse_position(sfen).ok_or_else(|| SearchError::InvalidSfen(sfen.to_owned()))?;
    let required = multipv.min(all_legal_moves_partial(&parsed).len());
    if legal_distinct_entry_count(sfen, &position.entries) < required {
        return Ok((!inflight.contains(sfen)).then(|| LeafPath {
            steps: Vec::new(),
            leaf_sfen: sfen.to_owned(),
        }));
    }

    let turn = match parsed.side_to_move() {
        shogi_core::Color::Black => "black",
        shogi_core::Color::White => "white",
    };
    let filtered: Vec<&BookEntry> = match filter {
        Some(filter) if filter.book_side == Some(turn) => {
            best_eval_entry(&position.entries, random.as_deref_mut())
                .into_iter()
                .collect()
        }
        Some(filter) => {
            let relative_threshold = filter
                .eval_diff
                .map(|eval_diff| filter.root_best_eval.saturating_sub(eval_diff));
            let threshold = match (relative_threshold, filter.min_eval_cp) {
                (Some(relative), Some(absolute)) => relative.max(absolute),
                (Some(relative), None) => relative,
                (None, Some(absolute)) => absolute,
                (None, None) => i32::MIN,
            };
            position
                .entries
                .iter()
                .filter(|entry| entry.eval_cp >= threshold)
                .collect()
        }
        None => position.entries.iter().collect(),
    };
    if filtered.is_empty() {
        dead.insert(sfen.to_owned());
        return Ok(None);
    }
    let parent_visits = position.entries.iter().map(|entry| entry.visits).sum();
    let mut candidates = Vec::with_capacity(filtered.len());
    for entry in filtered {
        let child_sfen = child_sfen(book, sfen, &entry.move_usi)?;
        let ucb = calculate_ucb(
            entry.eval_cp,
            entry.visits,
            parent_visits,
            c_puct,
            eval_scale,
        )?;
        candidates.push((ucb, entry.order_index, &entry.move_usi, child_sfen));
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    visiting.insert(sfen.to_owned());
    for (_, _, move_usi, child_sfen) in candidates {
        if let Some(mut child_path) = select_available_leaf(
            book,
            &child_sfen,
            multipv,
            c_puct,
            eval_scale,
            inflight,
            max_ply,
            filter,
            random.as_deref_mut(),
            depth + 1,
            visiting,
            dead,
        )? {
            visiting.remove(sfen);
            child_path.steps.insert(0, PathStep::new(sfen, move_usi));
            return Ok(Some(child_path));
        }
    }
    visiting.remove(sfen);
    dead.insert(sfen.to_owned());
    Ok(None)
}
fn best_eval_entry<'a>(
    entries: &'a [BookEntry],
    random: Option<&mut PythonRandom>,
) -> Option<&'a BookEntry> {
    let best_eval = entries.iter().map(|entry| entry.eval_cp).max()?;
    let tied: Vec<_> = entries
        .iter()
        .filter(|entry| entry.eval_cp == best_eval)
        .collect();
    let index = random
        .and_then(|random| random.choice_index(tied.len()))
        .unwrap_or(0);
    tied.get(index).copied()
}
fn child_sfen<B: SearchBook + ?Sized>(
    book: &B,
    sfen: &str,
    move_usi: &str,
) -> Result<String, SearchError> {
    let parsed = parse_position(sfen).ok_or_else(|| SearchError::InvalidSfen(sfen.to_owned()))?;
    let move_value =
        parse_move(move_usi, parsed.side_to_move()).ok_or_else(|| SearchError::IllegalMove {
            sfen: sfen.to_owned(),
            move_usi: move_usi.to_owned(),
        })?;
    if is_legal_partial(&parsed, move_value).is_err() {
        return Err(SearchError::IllegalMove {
            sfen: sfen.to_owned(),
            move_usi: move_usi.to_owned(),
        });
    }
    let mut child = parsed;
    child
        .make_move(move_value)
        .ok_or_else(|| SearchError::IllegalMove {
            sfen: sfen.to_owned(),
            move_usi: move_usi.to_owned(),
        })?;
    Ok(book.search_position_key(&child.to_sfen_owned()))
}
