#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusPosition {
    pub sfen: String,
    pub move_usi: String,
    pub ply: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedGame {
    pub source_path: String,
    pub sha256: String,
    pub players: [String; 2],
    pub moves: Vec<String>,
    pub positions: Vec<CorpusPosition>,
    pub endgame: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordErrorKind {
    Decode,
    TurnBeforePosition,
    TimeBeforeFirstMove,
    ScoreBeforeFirstMove,
    UnsupportedPosition,
    InvalidPositionToken,
    NonStartpos,
    InvalidMove,
    IllegalMove,
    NoMoves,
    MissingEndgame,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{source_path}{line_suffix}: {detail}")]
pub struct RecordError {
    pub source_path: Box<str>,
    pub kind: RecordErrorKind,
    pub line: Option<usize>,
    pub previous_sfen: Option<Box<str>>,
    pub move_text: Option<Box<str>>,
    pub detail: Box<str>,
    line_suffix: Box<str>,
}

impl RecordError {
    pub fn new(
        source_path: &str,
        kind: RecordErrorKind,
        line: Option<usize>,
        detail: impl Into<String>,
    ) -> Self {
        let line_suffix = line.map_or_else(String::new, |value| format!(": line {value}"));
        Self {
            source_path: source_path.to_owned().into_boxed_str(),
            kind,
            line,
            previous_sfen: None,
            move_text: None,
            detail: detail.into().into_boxed_str(),
            line_suffix: line_suffix.into_boxed_str(),
        }
    }

    pub fn with_position(mut self, sfen: String, move_text: &str) -> Self {
        self.previous_sfen = Some(sfen.into_boxed_str());
        self.move_text = Some(move_text.to_owned().into_boxed_str());
        self
    }
}
