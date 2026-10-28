//! The AI half: turn explanations in plain words into hypotheses that bet on
//! every condition. The model writes its bets before a single experiment
//! runs, and it never sees a result.

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::case::Case;
use crate::hypothesis::{self, Hypothesis, Origin};
use crate::openai::{OpenAi, Usage};

pub fn instructions(extra: usize) -> String {
    let own = match extra {
        0 => String::new(),
        1 => " Then propose one hypothesis of your own: a different cause that also \
              fits the facts, whose bets differ from every other hypothesis under at \
              least one condition (otherwise no experiment can tell them apart). Set \
              its explanation to null."
            .into(),
        n => format!(
            " Then propose {n} hypotheses of your own: different causes that also fit \
             the facts, each with bets that differ from every other hypothesis under at \
             least one condition (otherwise no experiment can tell them apart). Set \
             their explanation to null."
        ),
    };
    format!(
        "You turn explanations for a failing test into hypotheses that can be tested.\n\
         \n\
         For each numbered explanation, write exactly one hypothesis that keeps its \
         meaning, even if you think it is wrong.{own}\n\
         \n\
         A hypothesis is a bet. For every condition, give the fraction of runs (0 to 1) \
         in which the test would fail if the hypothesis were true. Derive each number \
         from the mechanism, not from how much you believe the hypothesis. If a \
         hypothesis says a knob does not matter, bet the same rate wherever only that \
         knob changes.\n\
         \n\
         Also give your confidence (0 to 1) that each hypothesis is the real cause, \
         before any experiment; together they add up to at most 1. Confidence is shown \
         next to the results and never counts as evidence.\n\
         \n\
         name: two or three words. claim: one sentence."
    )
}

/// The case as the model reads it. Conditions get short ids (`c1`, `c2`, ...).
pub fn prompt(case: &Case) -> String {
    let mut p = format!("Question: {}\n\nFacts:\n", case.question);
    for fact in &case.facts {
        let _ = writeln!(p, "- {fact}");
    }
    let test = &case.test;
    let _ = write!(
        p,
        "\nFailing test: {}\n{}\n\nTest source ({}):\n{}\n\nExplanations:\n",
        test.name,
        test.failure.trim_end(),
        test.file,
        test.source.trim_end()
    );
    for (i, explanation) in case.explanations.iter().enumerate() {
        let _ = writeln!(p, "{}. {explanation}", i + 1);
    }
    p.push_str("\nKnobs:\n");
    for knob in &case.knobs {
        let _ = writeln!(
            p,
            "- {} = {}: {}",
            knob.name,
            knob.values.join(" | "),
            knob.about
        );
    }
    p.push_str("\nConditions:\n");
    for (i, condition) in case.conditions().iter().enumerate() {
        let _ = writeln!(p, "c{}: {condition}", i + 1);
    }
    p
}

/// Strict JSON schema for the answer: every field required, nothing extra.
pub fn schema(case: &Case) -> Value {
    let ids: Vec<String> = (1..=case.conditions().len())
        .map(|i| format!("c{i}"))
        .collect();
    json!({
        "type": "object",
        "properties": {
            "hypotheses": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "explanation": { "type": ["integer", "null"] },
                        "name": { "type": "string" },
                        "claim": { "type": "string" },
                        "confidence": { "type": "number" },
                        "bets": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "condition": { "type": "string", "enum": ids },
                                    "fail_rate": { "type": "number" }
                                },
                                "required": ["condition", "fail_rate"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["explanation", "name", "claim", "confidence", "bets"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["hypotheses"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
pub struct Reply {
    pub hypotheses: Vec<Draft>,
}

#[derive(Debug, Deserialize)]
pub struct Draft {
    pub explanation: Option<usize>,
    pub name: String,
    pub claim: String,
    pub confidence: f64,
    pub bets: Vec<Bet>,
}

#[derive(Debug, Deserialize)]
pub struct Bet {
    pub condition: String,
    pub fail_rate: f64,
}

pub async fn hypothesize(
    client: &OpenAi,
    model: &str,
    case: &Case,
    extra: usize,
) -> Result<(Vec<Hypothesis>, Usage)> {
    let (reply, usage): (Reply, Usage) = client
        .structured(
            model,
            &instructions(extra),
            &prompt(case),
            "hypotheses",
            schema(case),
        )
        .await?;
    Ok((from_reply(reply, case, extra)?, usage))
}

/// The model's answer is a claim too: one hypothesis per explanation, the
/// number of extras we asked for, one bet per condition. Ids follow the
/// explanations; the model's own come last.
pub fn from_reply(reply: Reply, case: &Case, extra: usize) -> Result<Vec<Hypothesis>> {
    let conditions = case.conditions();
    let explained = case.explanations.len();
    let mut drafts = reply.hypotheses;
    for n in 1..=explained {
        let count = drafts.iter().filter(|d| d.explanation == Some(n)).count();
        if count != 1 {
            bail!("the model wrote {count} hypotheses for explanation {n}, not 1");
        }
    }
    if let Some(d) = drafts
        .iter()
        .find(|d| d.explanation.is_some_and(|n| n == 0 || n > explained))
    {
        bail!(
            "the model refers to explanation {:?}, which doesn't exist",
            d.explanation
        );
    }
    let own = drafts.iter().filter(|d| d.explanation.is_none()).count();
    if own != extra {
        bail!("asked for {extra} hypotheses of the model's own, got {own}");
    }
    drafts.sort_by_key(|d| d.explanation.unwrap_or(usize::MAX));

    let mut hypotheses = Vec::new();
    for (i, draft) in drafts.into_iter().enumerate() {
        let id = format!("H{}", i + 1);
        let mut bets = BTreeMap::new();
        for bet in draft.bets {
            let index = bet
                .condition
                .strip_prefix('c')
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| (1..=conditions.len()).contains(n));
            let Some(index) = index else {
                bail!("{id} bets on unknown condition {:?}", bet.condition);
            };
            let key = conditions[index - 1].key();
            if bets.insert(key, bet.fail_rate).is_some() {
                bail!("{id} bets twice on {}", bet.condition);
            }
        }
        hypotheses.push(Hypothesis {
            id,
            name: draft.name,
            claim: draft.claim,
            origin: draft.explanation.map_or(Origin::Model, Origin::Explanation),
            confidence: Some(draft.confidence),
            bets,
        });
    }
    hypothesis::validate(&hypotheses, case)?;
    Ok(hypotheses)
}
