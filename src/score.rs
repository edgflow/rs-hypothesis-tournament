//! Scoring by evidence. Each hypothesis bet on a fail rate; an experiment saw
//! `failed` of `runs` fail. The closer a bet was to what happened, the higher
//! its likelihood. The model's confidence plays no part.

/// No bet is ever certain: a rate of exactly 0 or 1 would let one surprising
/// run wipe a hypothesis out for good. Bets are clamped to 2%..98%.
pub const EPSILON: f64 = 0.02;

pub fn clamp(rate: f64) -> f64 {
    rate.clamp(EPSILON, 1.0 - EPSILON)
}

/// ln P(this outcome | the bet). The binomial coefficient is the same for
/// every hypothesis, so it is left out.
pub fn log_likelihood(rate: f64, failed: u32, runs: u32) -> f64 {
    let p = clamp(rate);
    f64::from(failed) * p.ln() + f64::from(runs - failed) * (1.0 - p).ln()
}

/// Turns log-likelihoods into shares that add up to 1: everyone starts equal.
pub fn shares(log_likelihoods: &[f64]) -> Vec<f64> {
    let best = log_likelihoods
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = log_likelihoods.iter().map(|l| (l - best).exp()).collect();
    let total: f64 = weights.iter().sum();
    weights.iter().map(|w| w / total).collect()
}

/// What one run under a condition would tell us, in bits: the uncertainty of
/// the pooled bet minus each hypothesis's own. Zero when every bet agrees.
pub fn information(shares: &[f64], rates: &[f64]) -> f64 {
    let pooled: f64 = shares.iter().zip(rates).map(|(w, r)| w * clamp(*r)).sum();
    let own: f64 = shares
        .iter()
        .zip(rates)
        .map(|(w, r)| w * entropy(clamp(*r)))
        .sum();
    (entropy(pooled) - own).max(0.0)
}

/// Binary entropy in bits.
fn entropy(p: f64) -> f64 {
    if p <= 0.0 || p >= 1.0 {
        return 0.0;
    }
    -(p * p.log2() + (1.0 - p) * (1.0 - p).log2())
}
