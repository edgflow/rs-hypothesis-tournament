mod common;

use hypothesis_tournament::hypothesis::{self, Hypothesis, HypothesisFile, Origin};

#[test]
fn every_condition_needs_exactly_one_bet() {
    let case = common::case();
    let mut h = common::hypothesis("H1", &case, |_| 0.2);
    h.bets.remove("threads=8 tz=UTC");
    let err = hypothesis::validate(&[h], &case).unwrap_err().to_string();
    assert!(err.contains("no bet on `threads=8 tz=UTC`"), "{err}");

    let mut h = common::hypothesis("H1", &case, |_| 0.2);
    h.bets.insert("threads=4 tz=UTC".into(), 0.1);
    let err = hypothesis::validate(&[h], &case).unwrap_err().to_string();
    assert!(err.contains("not a condition"), "{err}");
}

#[test]
fn a_fail_rate_is_between_zero_and_one() {
    let case = common::case();
    let h = common::hypothesis("H1", &case, |_| 1.5);
    assert!(hypothesis::validate(&[h], &case).is_err());

    let mut h = common::hypothesis("H1", &case, |_| 0.5);
    h.confidence = Some(-0.1);
    assert!(hypothesis::validate(&[h], &case).is_err());
}

#[test]
fn the_baseline_starts_at_the_ci_rate_everywhere() {
    let case = common::case();
    let h = Hypothesis::just_flaky(&case);
    assert_eq!(h.origin, Origin::Baseline);
    assert_eq!(h.confidence, None);
    assert!(h.bets.values().all(|&r| r == 0.25));
    hypothesis::validate(&[h], &case).unwrap();
}

#[test]
fn the_example_hypotheses_load() {
    let case =
        hypothesis_tournament::case::Case::load("examples/checkout/case.json".as_ref()).unwrap();
    let file: HypothesisFile =
        hypothesis::load("examples/checkout/hypotheses.json".as_ref(), &case).unwrap();
    assert_eq!(file.hypotheses.len(), 4);
    assert_eq!(file.hypotheses[3].origin, Origin::Model);
}
