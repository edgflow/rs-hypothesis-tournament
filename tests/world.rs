mod common;

use hypothesis_tournament::world::{Command, Simulated, World};

#[test]
fn the_same_seed_gives_the_same_evidence() {
    let case = common::case();
    let truth = common::truth(&case, |_| 0.5);
    let condition = &case.conditions()[0];
    let runs = |seed| {
        let mut world = Simulated::new(truth.clone(), seed);
        (0..40)
            .map(|_| world.run_once(condition).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(runs(7), runs(7));
    assert_ne!(runs(7), runs(8));
    let failed = runs(7).iter().filter(|f| **f).count();
    assert!((10..=30).contains(&failed), "{failed} of 40");
}

#[test]
fn a_rate_of_zero_never_fails_and_one_always_does() {
    let case = common::case();
    let mut world = Simulated::new(
        common::truth(&case, |c| if common::utc(c) { 1.0 } else { 0.0 }),
        1,
    );
    for condition in case.conditions() {
        for _ in 0..50 {
            assert_eq!(world.run_once(&condition).unwrap(), common::utc(&condition));
        }
    }
}

#[test]
fn the_example_truth_covers_every_condition() {
    let case =
        hypothesis_tournament::case::Case::load("examples/checkout/case.json".as_ref()).unwrap();
    Simulated::load("examples/checkout/truth.json".as_ref(), &case, 7).unwrap();
}

#[test]
fn a_command_gets_every_knob_as_an_environment_variable() {
    let case = common::case();
    // "Fails" (exit 1) unless the run is single-threaded in UTC.
    let mut world = Command::new(r#"test "$THREADS" = 1 && test "$TZ" = UTC"#, ".");
    let failed: Vec<bool> = case
        .conditions()
        .iter()
        .map(|c| world.run_once(c).unwrap())
        .collect();
    assert_eq!(failed, [false, true, true, true]);
}
