use thiserror::Error;

use crate::search::SearchResult;

pub const MATE_SCORE: i32 = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionRoot {
    Startpos,
    Sfen(String),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum UsiParseError {
    #[error("invalid integer {value} after {field}")]
    InvalidInteger { field: String, value: String },
    #[error("unsupported score type: {0}")]
    UnsupportedScore(String),
}

pub fn build_position_command(root: &PositionRoot, moves: &[&str]) -> String {
    let root_text = match root {
        PositionRoot::Startpos => "startpos".to_owned(),
        PositionRoot::Sfen(sfen) => format!("sfen {sfen}"),
    };
    if moves.is_empty() {
        format!("position {root_text}")
    } else {
        format!("position {root_text} moves {}", moves.join(" "))
    }
}

pub fn parse_info_line(
    line: &str,
    allow_bound: bool,
) -> Result<Option<(usize, SearchResult)>, UsiParseError> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.first() != Some(&"info") {
        return Ok(None);
    }
    if !allow_bound && (tokens.contains(&"lowerbound") || tokens.contains(&"upperbound")) {
        return Ok(None);
    }
    let mut depth = 0;
    let mut multipv = 1;
    let mut eval_cp = None;
    let mut pv: &[&str] = &[];
    let mut index = 1;
    while index < tokens.len() {
        match tokens[index] {
            "depth" if index + 1 < tokens.len() => {
                depth = parse_integer("depth", tokens[index + 1])?;
                index += 2;
            }
            "multipv" if index + 1 < tokens.len() => {
                multipv = parse_integer("multipv", tokens[index + 1])?;
                index += 2;
            }
            "score" if index + 2 < tokens.len() => {
                eval_cp = Some(parse_score(tokens[index + 1], tokens[index + 2])?);
                index += 3;
            }
            "pv" => {
                pv = &tokens[index + 1..];
                break;
            }
            _ => index += 1,
        }
    }
    let (Some(eval_cp), Some(move_usi)) = (eval_cp, pv.first()) else {
        return Ok(None);
    };
    let response = pv.get(1).copied().unwrap_or("none");
    Ok(Some((
        multipv,
        SearchResult::new(*move_usi, response, eval_cp, depth),
    )))
}

pub fn parse_score(score_type: &str, value: &str) -> Result<i32, UsiParseError> {
    match score_type {
        "cp" => parse_integer("score cp", value),
        "mate" => match value {
            "+" => Ok(MATE_SCORE),
            "-" => Ok(-MATE_SCORE),
            _ => {
                let mate: i32 = parse_integer("score mate", value)?;
                if mate > 0 {
                    Ok(MATE_SCORE - mate)
                } else {
                    Ok(-MATE_SCORE - mate)
                }
            }
        },
        other => Err(UsiParseError::UnsupportedScore(other.to_owned())),
    }
}

fn parse_integer<T>(field: &str, value: &str) -> Result<T, UsiParseError>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| UsiParseError::InvalidInteger {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}
