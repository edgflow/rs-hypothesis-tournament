mod common;

use common::{case, hypothesis, parallel, truth, utc};
use hypothesis_tournament::hypothesis::Hypothesis;
use hypothesis_tournament::tournament::{self, Report, Rules, Verdict};
use hypothesis_tournament::world::Simulated;

fn play(
    field: &[Hypothesis],
    rates: &dyn Fn(&hypothesis_tournament::case::Condition) -> f64,
    seed: u64,
) -> Report {
    let case = case();
    let mut world = Simulated::new(truth(&case, rates), seed);
    tournament::run(&case, field, &mut world, Rules::default(), |_| {}).unwrap()
}

fn field() -> Vec<Hypothesis> {
    let case = case();
    vec![
        Hypothesis::just_flaky(&case),
        hypothesis("H1", &case, |c| if utc(c) { 0.5 } else { 0.0 }),
        hypothesis("H2", &case, |c| if parallel(c) { 0.5 } else { 0.0 }),
    ]
}

#[test]
fn the_true_explanation_wins() {
    let report = play(&field(), &|c| if utc(c) { 0.5 } else { 0.0 }, 7);
    let Verdict::Winner { leader, log10_odds } = report.verdict else {
        panic!("{:?}", report.verdict)
    };
    assert_eq!(leader, 1);
    assert!(log10_odds >= 2.0);
}

#[test]
fn the_winner_does_not_depend_on_the_seed() {
    for seed in 0..20 {
        let report = play(&field(), &|c| if parallel(c) { 0.5 } else { 0.0 }, seed);
        assert!(
            matches!(report.verdict, Verdict::Winner { leader: 2, .. }),
            "seed {seed}: {:?}",
            report.verdict
        );
    }
}

#[test]
fn when_no_knob_matters_the_baseline_wins() {
    let report = play(&field(), &|_| 0.3, 7);
    assert!(
        matches!(report.verdict, Verdict::Winner { leader: 0, .. }),
        "{:?}",
        report.verdict
    );
}

#[test]
fn failing_where_it_bet_is_not_enough() {
    // The test fails 100% in UTC. After one round that's also "fails at a fixed
    // rate of 100%", so the winner must also pass where it bet it would.
    let case = case();
    let field = vec![
        Hypothesis::just_flaky(&case),
        hypothesis("H1", &case, |c| if utc(c) { 1.0 } else { 0.0 }),
    ];
    let report = play(&field, &|c| if utc(c) { 1.0 } else { 0.0 }, 7);
    assert!(matches!(report.verdict, Verdict::Winner { leader: 1, .. }));
    assert_eq!(report.rounds.len(), 2);
    assert_eq!(report.rounds[0].failed(), 20);
    assert_eq!(report.rounds[1].failed(), 0);
    assert!(!report.rounds[0].out[0], "the baseline survives round one");
}

#[test]
fn identical_bets_are_a_tie_not_a_guess() {
    let case = case();
    let mut twin = hypothesis("H3", &case, |c| if utc(c) { 0.5 } else { 0.0 });
    twin.confidence = Some(0.9);
    let mut field = field();
    field.push(twin);
    let report = play(&field, &|c| if utc(c) { 0.5 } else { 0.0 }, 7);
    assert_eq!(
        report.verdict,
        Verdict::Tie {
            leaders: vec![1, 3]
        }
    );
}

#[test]
fn confidence_never_changes_the_result() {
    let mut sure = field();
    sure[1].confidence = Some(0.01);
    sure[2].confidence = Some(0.99);
    let rates = |c: &hypothesis_tournament::case::Condition| if utc(c) { 0.5 } else { 0.0 };
    let a = play(&field(), &rates, 3);
    let b = play(&sure, &rates, 3);
    assert_eq!(a.verdict, b.verdict);
    assert_eq!(a.log_likelihoods, b.log_likelihoods);
}

#[test]
fn every_experiment_separates_the_hypotheses_left() {
    let report = play(&field(), &|c| if utc(c) { 0.5 } else { 0.0 }, 7);
    for round in &report.rounds {
        assert!(round.bits > 0.0);
        assert_eq!(round.results.len(), 20);
    }
}

#[test]
fn out_of_rounds_is_undecided() {
    let case = case();
    let field = vec![
        hypothesis("H1", &case, |c| if utc(c) { 0.30 } else { 0.0 }),
        hypothesis("H2", &case, |c| if utc(c) { 0.36 } else { 0.0 }),
    ];
    let mut world = Simulated::new(truth(&case, |c| if utc(c) { 0.33 } else { 0.0 }), 7);
    let rules = Rules {
        max_rounds: 2,
        ..Rules::default()
    };
    let report = tournament::run(&case, &field, &mut world, rules, |_| {}).unwrap();
    assert!(
        matches!(report.verdict, Verdict::Undecided { .. }),
        "{:?}",
        report.verdict
    );
    assert_eq!(report.rounds.len(), 2);
}
