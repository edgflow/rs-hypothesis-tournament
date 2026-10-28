//! The tournament. Each round picks the experiment that best separates the
//! hypotheses still in the game, runs it, and scores every bet against what
//! happened. A hypothesis is out once it is `margin` times less likely than
//! the leader. The last one standing wins.

use anyhow::{Result, bail};

use crate::case::{Case, Condition};
use crate::hypothesis::{Hypothesis, Origin};
use crate::score;
use crate::world::World;

/// Contenders whose bets never differ by more than this can't be told apart
/// with these knobs: it would take hundreds of runs.
pub const MIN_GAP: f64 = 0.05;

#[derive(Debug, Clone, Copy)]
pub struct Rules {
    /// Runs per experiment.
    pub runs: u32,
    pub max_rounds: u32,
    /// A hypothesis is out once it is this many times less likely than the leader.
    pub margin: f64,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            runs: 20,
            max_rounds: 8,
            margin: 100.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Round {
    pub number: u32,
    pub condition: Condition,
    /// What one run was expected to tell us, in bits, when the experiment was chosen.
    pub bits: f64,
    /// Every run in order: `true` means the test failed.
    pub results: Vec<bool>,
    /// Each hypothesis's share of the evidence after this round.
    pub shares: Vec<f64>,
    /// Which hypotheses are out after this round.
    pub out: Vec<bool>,
}

impl Round {
    pub fn failed(&self) -> usize {
        self.results.iter().filter(|failed| **failed).count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Every rival is out. `log10_odds`: how much more likely than the runner-up.
    Winner { leader: usize, log10_odds: f64 },
    /// No experiment can separate these: their bets agree under every condition.
    Tie { leaders: Vec<usize> },
    /// The round budget ran out first.
    Undecided { leader: usize, log10_odds: f64 },
}

#[derive(Debug)]
pub struct Report {
    pub rounds: Vec<Round>,
    pub log_likelihoods: Vec<f64>,
    pub verdict: Verdict,
}

pub fn run(
    case: &Case,
    hypotheses: &[Hypothesis],
    world: &mut dyn World,
    rules: Rules,
    mut on_round: impl FnMut(&Round),
) -> Result<Report> {
    if hypotheses.len() < 2 {
        bail!("a tournament needs at least two hypotheses");
    }
    let conditions = case.conditions();
    let mut evidence: Vec<Evidence> = Vec::new();
    let mut out = vec![false; hypotheses.len()];
    let mut rounds: Vec<Round> = Vec::new();
    let verdict = loop {
        let ll = log_likelihoods(hypotheses, &evidence);
        let leader = best(&ll);
        let alive: Vec<usize> = (0..ll.len()).filter(|&i| !out[i]).collect();
        if alive.len() == 1 {
            break Verdict::Winner {
                leader,
                log10_odds: odds(&ll, leader),
            };
        }
        if rounds.len() as u32 == rules.max_rounds {
            break Verdict::Undecided {
                leader,
                log10_odds: odds(&ll, leader),
            };
        }
        let rates = |condition: &Condition| -> Vec<f64> {
            alive
                .iter()
                .map(|&i| bet(&hypotheses[i], condition, &evidence))
                .collect()
        };
        let shares = score::shares(&alive.iter().map(|&i| ll[i]).collect::<Vec<_>>());
        let Some((condition, bits)) = next_experiment(&conditions, &shares, rates) else {
            break Verdict::Tie { leaders: alive };
        };

        let results = (0..rules.runs)
            .map(|_| world.run_once(&condition))
            .collect::<Result<Vec<bool>>>()?;
        evidence.push(Evidence {
            condition: condition.clone(),
            failed: results.iter().filter(|f| **f).count() as u32,
            runs: rules.runs,
        });
        let ll = log_likelihoods(hypotheses, &evidence);
        let top = ll[best(&ll)];
        for (i, l) in ll.iter().enumerate() {
            out[i] |= *l < top - rules.margin.ln();
        }

        let round = Round {
            number: rounds.len() as u32 + 1,
            condition,
            bits,
            results,
            shares: score::shares(&ll),
            out: out.clone(),
        };
        on_round(&round);
        rounds.push(round);
    };
    Ok(Report {
        log_likelihoods: log_likelihoods(hypotheses, &evidence),
        rounds,
        verdict,
    })
}

/// One experiment: where it ran, and how many of its runs failed.
struct Evidence {
    condition: Condition,
    failed: u32,
    runs: u32,
}

/// How well each hypothesis's bets predicted all the evidence so far.
fn log_likelihoods(hypotheses: &[Hypothesis], evidence: &[Evidence]) -> Vec<f64> {
    hypotheses
        .iter()
        .map(|h| {
            evidence
                .iter()
                .map(|e| score::log_likelihood(bet(h, &e.condition, evidence), e.failed, e.runs))
                .sum()
        })
        .collect()
}

/// A hypothesis's bet. The baseline bets the one rate that fits all the
/// evidence best: its strongest case. To beat it, an explanation has to show
/// that a knob matters.
fn bet(h: &Hypothesis, condition: &Condition, evidence: &[Evidence]) -> f64 {
    let runs: u32 = evidence.iter().map(|e| e.runs).sum();
    if h.origin == Origin::Baseline && runs > 0 {
        f64::from(evidence.iter().map(|e| e.failed).sum::<u32>()) / f64::from(runs)
    } else {
        h.bet(condition)
    }
}

/// The condition where the bets of the hypotheses still in the game disagree
/// most, weighted by how likely each one is now. `None` when no condition
/// separates them by at least `MIN_GAP`.
fn next_experiment(
    conditions: &[Condition],
    shares: &[f64],
    rates: impl Fn(&Condition) -> Vec<f64>,
) -> Option<(Condition, f64)> {
    let mut best: Option<(Condition, f64)> = None;
    for condition in conditions {
        let rates: Vec<f64> = rates(condition).into_iter().map(score::clamp).collect();
        let gap =
            rates.iter().copied().fold(0.0, f64::max) - rates.iter().copied().fold(1.0, f64::min);
        if gap < MIN_GAP {
            continue;
        }
        let bits = score::information(shares, &rates);
        if best.as_ref().is_none_or(|(_, b)| bits > *b) {
            best = Some((condition.clone(), bits));
        }
    }
    best
}

fn best(ll: &[f64]) -> usize {
    (0..ll.len())
        .max_by(|&a, &b| ll[a].total_cmp(&ll[b]).then(b.cmp(&a)))
        .expect("at least two hypotheses")
}

/// log10 of how many times more likely the leader is than the best of the rest.
fn odds(ll: &[f64], leader: usize) -> f64 {
    let runner_up = (0..ll.len())
        .filter(|&i| i != leader)
        .map(|i| ll[i])
        .fold(f64::NEG_INFINITY, f64::max);
    (ll[leader] - runner_up) / std::f64::consts::LN_10
}
