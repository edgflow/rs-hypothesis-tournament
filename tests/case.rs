mod common;

use hypothesis_tournament::case::Case;

#[test]
fn conditions_are_every_combination_last_knob_fastest() {
    let keys: Vec<String> = common::case()
        .conditions()
        .iter()
        .map(|c| c.key())
        .collect();
    assert_eq!(
        keys,
        [
            "threads=1 tz=UTC",
            "threads=1 tz=Pacific/Auckland",
            "threads=8 tz=UTC",
            "threads=8 tz=Pacific/Auckland",
        ]
    );
}

#[test]
fn a_knob_needs_two_values_and_clean_names() {
    let mut case = common::case();
    case.knobs[0].values.pop();
    assert!(
        case.validate()
            .unwrap_err()
            .to_string()
            .contains("at least two values")
    );

    let mut case = common::case();
    case.knobs[1].values[1] = "Pacific Auckland".into();
    assert!(case.validate().unwrap_err().to_string().contains("' '"));

    let mut case = common::case();
    case.knobs.push(common::knob("tz", &["a", "b"]));
    assert!(case.validate().unwrap_err().to_string().contains("twice"));
}

#[test]
fn too_many_conditions_are_refused() {
    let mut case = common::case();
    for name in ["a", "b", "c", "d", "e"] {
        case.knobs.push(common::knob(name, &["x", "y"]));
    }
    assert!(
        case.validate()
            .unwrap_err()
            .to_string()
            .contains("128 conditions")
    );
}

#[test]
fn the_example_case_loads_with_its_test_source() {
    let case = Case::load("examples/checkout/case.json".as_ref()).unwrap();
    assert_eq!(case.conditions().len(), 8);
    assert!(
        case.test
            .source
            .contains("fn promo_code_takes_ten_percent_off")
    );
    assert_eq!(case.ci_rate(), 9.0 / 40.0);
}
