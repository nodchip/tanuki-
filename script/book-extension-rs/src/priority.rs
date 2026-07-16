use std::cmp::Ordering;
use thiserror::Error;

pub const POLICY_VERSION: &str = "corpus-priority-v2";
const SITES: [&str; 3] = ["wcsc", "denryu", "floodgate"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualityFacts {
    pub event_year: i32,
    pub strength_tier: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualityDecision {
    pub age_distance: i32,
    pub quality_band: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OfficialStrength {
    SingleStage { rank: i32, participants: i32 },
    TopStage { rank: i32, participants: i32 },
    OneStageBelow { rank: i32, participants: i32 },
    TwoOrMoreStagesBelow,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloodgateStrength {
    pub high_reliability: bool,
    pub connected_to_anchor: bool,
    pub enough_effective_games: bool,
    pub snapshot_percentile: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PriorityFacts {
    pub year: i32,
    pub stage_tier: i32,
    pub mover_percentile: f64,
    pub opponent_stage_tier: i32,
    pub opponent_percentile: f64,
    pub rating_reliability: i32,
    pub anchor_margin: f64,
    pub rating_percentile: f64,
    pub recent_occurrences: i64,
    pub occurrences: i64,
    pub exact_time: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct PriorityTuple([f64; 11]);

impl PartialEq for PriorityTuple {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for PriorityTuple {}
impl PartialOrd for PriorityTuple {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PriorityTuple {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .iter()
            .zip(other.0.iter())
            .map(|(left, right)| left.total_cmp(right))
            .find(|ordering| *ordering != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PriorityError {
    #[error("year must be positive")]
    InvalidYear,
    #[error("invalid quality facts")]
    InvalidQualityFacts,
    #[error("weights must define three non-negative sites")]
    InvalidWeights,
    #[error("invalid site or node count")]
    InvalidRecord,
    #[error("priority component is outside the fixed-width encoding range")]
    InvalidComponent,
}

pub fn quality_decision(
    priority_reference_year: i32,
    facts: &QualityFacts,
) -> Result<QualityDecision, PriorityError> {
    if priority_reference_year <= 0
        || facts.event_year <= 0
        || !(0..=4).contains(&facts.strength_tier)
    {
        return Err(PriorityError::InvalidQualityFacts);
    }
    let age_distance = priority_reference_year
        .saturating_sub(facts.event_year)
        .div_euclid(3);
    let quality_band = facts
        .strength_tier
        .checked_add(age_distance.saturating_mul(2))
        .ok_or(PriorityError::InvalidQualityFacts)?;
    Ok(QualityDecision {
        age_distance,
        quality_band,
    })
}

pub fn official_strength_tier(strength: OfficialStrength) -> i32 {
    let valid = |rank: i32, participants: i32| rank > 0 && participants > 0 && rank <= participants;
    let top_quarter = |rank: i32, participants: i32| i64::from(rank) * 4 <= i64::from(participants);
    let top_half = |rank: i32, participants: i32| i64::from(rank) * 2 <= i64::from(participants);

    match strength {
        OfficialStrength::SingleStage { rank, participants } if valid(rank, participants) => {
            if rank == 1 {
                0
            } else if rank <= 3 {
                1
            } else if top_quarter(rank, participants) {
                2
            } else if top_half(rank, participants) {
                3
            } else {
                4
            }
        }
        OfficialStrength::TopStage { rank, participants } if valid(rank, participants) => {
            if rank == 1 {
                0
            } else if rank <= 3 {
                1
            } else {
                2
            }
        }
        OfficialStrength::OneStageBelow { rank, participants } if valid(rank, participants) => {
            if top_quarter(rank, participants) {
                3
            } else {
                4
            }
        }
        OfficialStrength::TwoOrMoreStagesBelow | OfficialStrength::Missing => 4,
        _ => 4,
    }
}

pub fn floodgate_strength_tier(strength: FloodgateStrength) -> i32 {
    if !strength.high_reliability
        || !strength.connected_to_anchor
        || !strength.enough_effective_games
        || !strength.snapshot_percentile.is_finite()
        || !(0.0..=1.0).contains(&strength.snapshot_percentile)
    {
        return 4;
    }
    if strength.snapshot_percentile >= 0.99 {
        1
    } else if strength.snapshot_percentile >= 0.95 {
        2
    } else if strength.snapshot_percentile >= 0.80 {
        3
    } else {
        4
    }
}
pub fn recency_bucket(year: i32) -> Result<i32, PriorityError> {
    if year < 1 {
        return Err(PriorityError::InvalidYear);
    }
    Ok((year - 2024).div_euclid(3))
}

pub fn priority_tuple(facts: &PriorityFacts) -> Result<PriorityTuple, PriorityError> {
    let reliable = facts.rating_reliability >= 2;
    Ok(PriorityTuple([
        f64::from(recency_bucket(facts.year)?),
        f64::from(facts.stage_tier),
        facts.mover_percentile,
        f64::from(facts.opponent_stage_tier),
        facts.opponent_percentile,
        f64::from(facts.rating_reliability),
        if reliable { facts.anchor_margin } else { 0.0 },
        if reliable {
            facts.rating_percentile
        } else {
            0.0
        },
        facts.recent_occurrences as f64,
        facts.occurrences as f64,
        facts.exact_time,
    ]))
}

pub fn encode_priority(tuple: &PriorityTuple) -> Result<[u8; 88], PriorityError> {
    let mut encoded = [0_u8; 88];
    for (index, value) in tuple.0.iter().copied().enumerate() {
        if !value.is_finite() {
            return Err(PriorityError::InvalidComponent);
        }
        let fixed = format!("{:.6}", value + 1_000_000.0);
        let (whole, fraction) = fixed
            .split_once('.')
            .ok_or(PriorityError::InvalidComponent)?;
        let whole = whole
            .parse::<u64>()
            .map_err(|_| PriorityError::InvalidComponent)?;
        let fraction = fraction
            .parse::<u64>()
            .map_err(|_| PriorityError::InvalidComponent)?;
        let scaled = whole
            .checked_mul(1_000_000)
            .and_then(|value| value.checked_add(fraction))
            .ok_or(PriorityError::InvalidComponent)?;
        encoded[index * 8..index * 8 + 8].copy_from_slice(&scaled.to_be_bytes());
    }
    Ok(encoded)
}

#[derive(Clone, Debug)]
pub struct SiteNodeBudget {
    weights: [u64; 3],
    nodes: [u64; 3],
}

impl SiteNodeBudget {
    pub fn new<const N: usize>(weights: [(&str, i64); N]) -> Result<Self, PriorityError> {
        if N != 3 {
            return Err(PriorityError::InvalidWeights);
        }
        let mut result = [None; 3];
        for (site, weight) in weights {
            let Some(index) = SITES.iter().position(|candidate| *candidate == site) else {
                return Err(PriorityError::InvalidWeights);
            };
            if weight < 0 || result[index].is_some() {
                return Err(PriorityError::InvalidWeights);
            }
            result[index] = Some(weight as u64);
        }
        let Some(weights) = result
            .map(|value| value)
            .into_iter()
            .collect::<Option<Vec<_>>>()
        else {
            return Err(PriorityError::InvalidWeights);
        };
        Ok(Self {
            weights: [weights[0], weights[1], weights[2]],
            nodes: [0; 3],
        })
    }

    pub fn order(&self) -> [&'static str; 3] {
        let mut indexes = [0usize, 1, 2];
        indexes.sort_by(|left, right| {
            let left_ratio = if self.weights[*left] == 0 {
                f64::INFINITY
            } else {
                self.nodes[*left] as f64 / self.weights[*left] as f64
            };
            let right_ratio = if self.weights[*right] == 0 {
                f64::INFINITY
            } else {
                self.nodes[*right] as f64 / self.weights[*right] as f64
            };
            left_ratio
                .total_cmp(&right_ratio)
                .then_with(|| left.cmp(right))
        });
        [SITES[indexes[0]], SITES[indexes[1]], SITES[indexes[2]]]
    }

    pub fn record(&mut self, site: &str, nodes: i64) -> Result<(), PriorityError> {
        let Some(index) = SITES.iter().position(|candidate| *candidate == site) else {
            return Err(PriorityError::InvalidRecord);
        };
        if nodes < 0 {
            return Err(PriorityError::InvalidRecord);
        }
        self.nodes[index] += nodes as u64;
        Ok(())
    }

    pub fn snapshot(&self) -> [(&'static str, u64); 3] {
        [
            (SITES[0], self.nodes[0]),
            (SITES[1], self.nodes[1]),
            (SITES[2], self.nodes[2]),
        ]
    }
}
