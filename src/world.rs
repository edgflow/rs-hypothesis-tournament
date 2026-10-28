//! Where experiments run. A world runs the failing test once under a
//! condition and says whether it failed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::case::{Case, Condition};

pub trait World {
    /// One line for the report: what produced the evidence.
    fn describe(&self) -> String;

    /// Runs the test once under `condition`. `Ok(true)`: the test failed.
    fn run_once(&mut self, condition: &Condition) -> Result<bool>;
}

/// Synthetic evidence: a hidden fail rate per condition and a seeded random
/// generator. Same seed, same tournament, on every machine.
pub struct Simulated {
    fail_rates: BTreeMap<String, f64>,
    rng: SplitMix64,
    label: String,
}

/// The hidden truth: never shown to the model or to the tournament.
#[derive(Deserialize)]
struct TruthFile {
    #[allow(dead_code)]
    cause: String,
    fail_rates: BTreeMap<String, f64>,
}

impl Simulated {
    pub fn new(fail_rates: BTreeMap<String, f64>, seed: u64) -> Self {
        Self {
            fail_rates,
            rng: SplitMix64(seed),
            label: format!("simulated, seed {seed}"),
        }
    }

    pub fn load(path: &Path, case: &Case, seed: u64) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let truth: TruthFile =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        for condition in case.conditions() {
            if !truth.fail_rates.contains_key(&condition.key()) {
                bail!("{} has no fail rate for `{condition}`", path.display());
            }
        }
        let mut world = Self::new(truth.fail_rates, seed);
        world.label = format!("simulated: {}, seed {seed}", path.display());
        Ok(world)
    }
}

impl World for Simulated {
    fn describe(&self) -> String {
        self.label.clone()
    }

    fn run_once(&mut self, condition: &Condition) -> Result<bool> {
        let rate = self.fail_rates[&condition.key()];
        Ok(self.rng.next_f64() < rate)
    }
}

/// A real test: runs a shell command once per run, with every knob as an
/// environment variable (`THREADS=8 TZ=UTC CLOCK=21:00`). Exit code 0 means
/// the test passed.
pub struct Command {
    script: String,
    dir: PathBuf,
}

impl Command {
    pub fn new(script: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        Self {
            script: script.into(),
            dir: dir.into(),
        }
    }
}

impl World for Command {
    fn describe(&self) -> String {
        format!("sh -c {:?}", self.script)
    }

    fn run_once(&mut self, condition: &Condition) -> Result<bool> {
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&self.script)
            .current_dir(&self.dir)
            .envs(condition.0.iter().map(|(k, v)| (k.to_uppercase(), v)))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("running {:?}", self.script))?;
        Ok(!status.success())
    }
}

/// SplitMix64: a tiny, well-known random generator. Same seed, same numbers.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1): the top 53 bits as a fraction.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}
