#![allow(dead_code)]

use std::collections::BTreeMap;

use hypothesis_tournament::case::{Case, CiHistory, Condition, FailingTest, Knob};
use hypothesis_tournament::hypothesis::{Hypothesis, Origin};

/// Two knobs, four conditions: threads 1|8 × tz UTC|Pacific/Auckland.
pub fn case() -> Case {
    Case {
        question: "Why does the promo test fail?".into(),
        facts: vec!["It fails on CI.".into()],
        test: FailingTest {
            name: "promo".into(),
            file: "promo.rs".into(),
            failure: "left: 100.0 right: 90.0".into(),
            source: "#[test] fn promo() {}".into(),
        },
        explanations: vec!["Time zone.".into(), "Race.".into()],
        knobs: vec![
            knob("threads", &["1", "8"]),
            knob("tz", &["UTC", "Pacific/Auckland"]),
        ],
        ci: CiHistory {
            runs: 40,
            failed: 10,
        },
    }
}

pub fn knob(name: &str, values: &[&str]) -> Knob {
    Knob {
        name: name.into(),
        about: format!("the {name}"),
        values: values.iter().map(|v| v.to_string()).collect(),
    }
}

/// A hypothesis whose bets come from a function of the condition.
pub fn hypothesis(id: &str, case: &Case, bet: impl Fn(&Condition) -> f64) -> Hypothesis {
    Hypothesis {
        id: id.into(),
        name: id.to_lowercase(),
        claim: format!("{id} is the cause."),
        origin: Origin::Explanation(1),
        confidence: Some(0.5),
        bets: case
            .conditions()
            .iter()
            .map(|c| (c.key(), bet(c)))
            .collect(),
    }
}

/// Fail rates for a simulated world, from a function of the condition.
pub fn truth(case: &Case, rate: impl Fn(&Condition) -> f64) -> BTreeMap<String, f64> {
    case.conditions()
        .iter()
        .map(|c| (c.key(), rate(c)))
        .collect()
}

pub fn utc(c: &Condition) -> bool {
    c.get("tz") == Some("UTC")
}

pub fn parallel(c: &Condition) -> bool {
    c.get("threads") == Some("8")
}
