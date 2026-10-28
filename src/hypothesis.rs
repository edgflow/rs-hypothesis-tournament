//! A hypothesis is a bet: for every condition, how often the test fails if
//! this explanation is true. A claim that won't bet can't be tested.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::case::{Case, Condition};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub name: String,
    pub claim: String,
    pub origin: Origin,
    /// How sure the model said it was, before any experiment. Shown next to
    /// the evidence, never used to score.
    pub confidence: Option<f64>,
    /// Condition key → the fraction of runs in which the test fails.
    pub bets: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Explanation number N from the case (counting from 1).
    Explanation(usize),
    /// Proposed by the model.
    Model,
    /// The baseline every explanation must beat.
    Baseline,
}

/// What `hypothesize` writes and `show` and `run` read.
#[derive(Debug, Serialize, Deserialize)]
pub struct HypothesisFile {
    pub model: String,
    pub hypotheses: Vec<Hypothesis>,
}

impl Hypothesis {
    /// The bet under one condition. `validate` makes sure there is one.
    pub fn bet(&self, condition: &Condition) -> f64 {
        self.bets[&condition.key()]
    }

    /// "It's just flaky": none of the knobs matter, the test fails at one
    /// fixed rate. It starts at the CI rate; in a tournament it bets whatever
    /// single rate fits the evidence best. An explanation that can't beat it
    /// explains nothing.
    pub fn just_flaky(case: &Case) -> Self {
        let rate = case.ci_rate();
        Self {
            id: "H0".into(),
            name: "just flaky".into(),
            claim: "None of the knobs matter: the test fails at one fixed rate.".into(),
            origin: Origin::Baseline,
            confidence: None,
            bets: case.conditions().iter().map(|c| (c.key(), rate)).collect(),
        }
    }
}

/// Every hypothesis must bet on every condition, and only on those.
pub fn validate(hypotheses: &[Hypothesis], case: &Case) -> Result<()> {
    let conditions = case.conditions();
    for (i, h) in hypotheses.iter().enumerate() {
        if hypotheses[..i].iter().any(|other| other.id == h.id) {
            bail!("two hypotheses are called {}", h.id);
        }
        for condition in &conditions {
            let Some(&rate) = h.bets.get(&condition.key()) else {
                bail!("{} places no bet on `{condition}`", h.id);
            };
            if !(0.0..=1.0).contains(&rate) {
                bail!(
                    "{} bets {rate} on `{condition}`: a fail rate is between 0 and 1",
                    h.id
                );
            }
        }
        if let Some(extra) = h
            .bets
            .keys()
            .find(|k| !conditions.iter().any(|c| &c.key() == *k))
        {
            bail!(
                "{} bets on `{extra}`, which is not a condition of this case",
                h.id
            );
        }
        if h.confidence.is_some_and(|c| !(0.0..=1.0).contains(&c)) {
            bail!("{}: confidence must be between 0 and 1", h.id);
        }
    }
    Ok(())
}

pub fn load(path: &Path, case: &Case) -> Result<HypothesisFile> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: HypothesisFile =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    validate(&file.hypotheses, case).with_context(|| format!("checking {}", path.display()))?;
    Ok(file)
}

pub fn save(path: &Path, file: &HypothesisFile) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(file)?;
    std::fs::write(path, json + "\n").with_context(|| format!("writing {}", path.display()))
}
