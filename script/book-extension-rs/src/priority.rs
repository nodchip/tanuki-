use std::cmp::Ordering;
use thiserror::Error;

pub const POLICY_VERSION: &str = "corpus-priority-v1";
const SITES: [&str; 3] = ["wcsc", "denryu", "floodgate"];

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
    #[error("weights must define three non-negative sites")]
    InvalidWeights,
    #[error("invalid site or node count")]
    InvalidRecord,
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
