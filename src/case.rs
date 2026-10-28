//! The case: a question about a failing test, what we already know, the
//! competing explanations, and the knobs an experiment can turn.

use std::fmt;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// More conditions than this and every hypothesis has too many bets to write.
const MAX_CONDITIONS: usize = 64;

#[derive(Debug, Clone, Deserialize)]
pub struct Case {
    pub question: String,
    /// Observations, not causes.
    pub facts: Vec<String>,
    pub test: FailingTest,
    /// The competing explanations, in plain words.
    pub explanations: Vec<String>,
    /// What an experiment can change. Every combination of values is one condition.
    pub knobs: Vec<Knob>,
    /// How often the test fails on CI today: the "just flaky" baseline.
    pub ci: CiHistory,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FailingTest {
    pub name: String,
    /// The test's source file, relative to the case file.
    pub file: String,
    pub failure: String,
    #[serde(skip)]
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Knob {
    pub name: String,
    pub about: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CiHistory {
    pub runs: u32,
    pub failed: u32,
}

/// One combination of knob values, in the case's knob order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition(pub Vec<(String, String)>);

impl Condition {
    /// `threads=8 tz=UTC load=busy`: the key in hypothesis and truth files.
    pub fn key(&self) -> String {
        self.to_string()
    }

    pub fn get(&self, knob: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| name == knob)
            .map(|(_, value)| value.as_str())
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (knob, value)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{knob}={value}")?;
        }
        Ok(())
    }
}

impl Case {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut case: Case =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let source = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&case.test.file);
        case.test.source = std::fs::read_to_string(&source)
            .with_context(|| format!("reading the test source {}", source.display()))?;
        case.validate()?;
        Ok(case)
    }

    pub fn validate(&self) -> Result<()> {
        if self.explanations.is_empty() {
            bail!("the case has no explanations to test");
        }
        if self.knobs.is_empty() {
            bail!("the case has no knobs: an experiment must be able to change something");
        }
        for (i, knob) in self.knobs.iter().enumerate() {
            if self.knobs[..i].iter().any(|k| k.name == knob.name) {
                bail!("knob {:?} is listed twice", knob.name);
            }
            if knob.values.len() < 2 {
                bail!("knob {:?} needs at least two values", knob.name);
            }
            for (j, value) in knob.values.iter().enumerate() {
                if knob.values[..j].contains(value) {
                    bail!("knob {:?} lists {value:?} twice", knob.name);
                }
            }
            // Keys look like `threads=8 tz=UTC`, so names and values can't hold ' ' or '='.
            let bad = |s: &str| s.is_empty() || s.contains([' ', '=']);
            if bad(&knob.name) || knob.values.iter().any(|v| bad(v)) {
                bail!(
                    "knob {:?}: names and values can't be empty or contain ' ' or '='",
                    knob.name
                );
            }
        }
        let count: usize = self.knobs.iter().map(|k| k.values.len()).product();
        if count > MAX_CONDITIONS {
            bail!("{count} conditions is too many (at most {MAX_CONDITIONS})");
        }
        if self.ci.runs == 0 || self.ci.failed > self.ci.runs {
            bail!("ci: failed must be between 0 and runs, and runs above 0");
        }
        Ok(())
    }

    /// Every combination of knob values; the last knob changes fastest.
    pub fn conditions(&self) -> Vec<Condition> {
        let mut all = vec![Condition(Vec::new())];
        for knob in &self.knobs {
            all = all
                .into_iter()
                .flat_map(|c| {
                    knob.values.iter().map(move |value| {
                        let mut next = c.0.clone();
                        next.push((knob.name.clone(), value.clone()));
                        Condition(next)
                    })
                })
                .collect();
        }
        all
    }

    /// How often the test fails on CI: the rate the "just flaky" baseline bets on.
    pub fn ci_rate(&self) -> f64 {
        f64::from(self.ci.failed) / f64::from(self.ci.runs)
    }
}
