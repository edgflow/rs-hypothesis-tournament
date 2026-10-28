//! The AI half: the prompt, the schema, the checks on the model's answer, and
//! a full request against a local stand-in for the OpenAI API.

mod common;

use std::time::Duration;

use hypothesis_tournament::generate::{self, Reply};
use hypothesis_tournament::hypothesis::Origin;
use hypothesis_tournament::openai::{OpenAi, OpenAiError};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A reply in the shape the schema asks for: `bets` maps c1..c4 to rates.
fn reply(drafts: &[(Option<usize>, [f64; 4])]) -> Value {
    let hypotheses: Vec<Value> = drafts
        .iter()
        .map(|(explanation, rates)| {
            let bets: Vec<Value> = rates
                .iter()
                .enumerate()
                .map(|(i, r)| json!({ "condition": format!("c{}", i + 1), "fail_rate": r }))
                .collect();
            json!({ "explanation": explanation, "name": "a name", "claim": "a claim.", "confidence": 0.3, "bets": bets })
        })
        .collect();
    json!({ "hypotheses": hypotheses })
}

fn parse(value: Value) -> Reply {
    serde_json::from_value(value).unwrap()
}

#[test]
fn the_prompt_numbers_explanations_and_conditions() {
    let prompt = generate::prompt(&common::case());
    assert!(prompt.contains("1. Time zone.\n2. Race.\n"));
    assert!(prompt.contains("- tz = UTC | Pacific/Auckland: the tz\n"));
    assert!(prompt.contains("c1: threads=1 tz=UTC\n"));
    assert!(prompt.contains("c4: threads=8 tz=Pacific/Auckland\n"));
    assert!(prompt.contains("#[test] fn promo() {}"));
}

#[test]
fn the_schema_only_allows_known_conditions() {
    let schema = generate::schema(&common::case());
    let bet = &schema["properties"]["hypotheses"]["items"]["properties"]["bets"]["items"];
    assert_eq!(
        bet["properties"]["condition"]["enum"],
        json!(["c1", "c2", "c3", "c4"])
    );
    assert_eq!(bet["additionalProperties"], false);
}

#[test]
fn bets_map_to_conditions_and_the_models_own_come_last() {
    let case = common::case();
    let value = reply(&[
        (None, [0.0, 0.0, 0.3, 0.3]),
        (Some(2), [0.0, 0.0, 0.25, 0.25]),
        (Some(1), [0.5, 0.0, 0.5, 0.0]),
    ]);
    let hypotheses = generate::from_reply(parse(value), &case, 1).unwrap();
    let ids: Vec<(&str, Origin)> = hypotheses
        .iter()
        .map(|h| (h.id.as_str(), h.origin))
        .collect();
    assert_eq!(
        ids,
        [
            ("H1", Origin::Explanation(1)),
            ("H2", Origin::Explanation(2)),
            ("H3", Origin::Model)
        ]
    );
    assert_eq!(hypotheses[0].bets["threads=1 tz=UTC"], 0.5);
    assert_eq!(hypotheses[0].bets["threads=8 tz=Pacific/Auckland"], 0.0);
    assert_eq!(hypotheses[0].confidence, Some(0.3));
}

#[test]
fn the_answer_is_checked_like_any_claim() {
    let case = common::case();
    let missing = reply(&[(Some(1), [0.5; 4])]);
    let err = generate::from_reply(parse(missing), &case, 0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("0 hypotheses for explanation 2"), "{err}");

    let extra = reply(&[(Some(1), [0.5; 4]), (Some(2), [0.5; 4]), (None, [0.5; 4])]);
    let err = generate::from_reply(parse(extra), &case, 0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("asked for 0"), "{err}");

    let mut twice = reply(&[(Some(1), [0.5; 4]), (Some(2), [0.5; 4])]);
    twice["hypotheses"][0]["bets"][1]["condition"] = json!("c1");
    let err = generate::from_reply(parse(twice), &case, 0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("bets twice on c1"), "{err}");

    let too_high = reply(&[(Some(1), [0.5, 0.5, 2.0, 0.5]), (Some(2), [0.5; 4])]);
    assert!(generate::from_reply(parse(too_high), &case, 0).is_err());
}

fn answer(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "status": "completed",
        "output": [
            { "type": "reasoning", "id": "rs_1", "summary": [] },
            { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": text }] }
        ],
        "usage": { "input_tokens": 900, "output_tokens": 4000, "total_tokens": 4900 }
    }))
}

fn client(server: &MockServer) -> OpenAi {
    OpenAi::new("test-key", server.uri()).with_retry_delay(Duration::from_millis(1))
}

#[tokio::test]
async fn one_request_with_a_strict_schema() {
    let server = MockServer::start().await;
    let text = reply(&[
        (Some(1), [0.5, 0.0, 0.5, 0.0]),
        (Some(2), [0.0, 0.0, 0.3, 0.3]),
    ])
    .to_string();
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(answer(&text))
        .expect(1)
        .mount(&server)
        .await;

    let case = common::case();
    let (hypotheses, usage) = generate::hypothesize(&client(&server), "gpt-test", &case, 0)
        .await
        .unwrap();
    assert_eq!(hypotheses.len(), 2);
    assert_eq!(usage.output_tokens, 4000);

    let body: Value = server.received_requests().await.unwrap()[0]
        .body_json()
        .unwrap();
    assert_eq!(body["model"], "gpt-test");
    assert_eq!(body["store"], false);
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["strict"], true);
    assert_eq!(body["input"], generate::prompt(&case));
    assert!(
        body["instructions"]
            .as_str()
            .unwrap()
            .contains("never counts as evidence")
    );
}

#[tokio::test]
async fn a_refusal_is_an_error_not_an_empty_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "completed",
            "output": [{ "type": "message", "content": [{ "type": "refusal", "refusal": "no" }] }]
        })))
        .mount(&server)
        .await;
    let err = generate::hypothesize(&client(&server), "gpt-test", &common::case(), 0)
        .await
        .unwrap_err();
    assert!(
        matches!(err.downcast_ref(), Some(OpenAiError::Refused(r)) if r == "no"),
        "{err}"
    );
}

#[tokio::test]
async fn retries_a_server_error_but_not_a_bad_key() {
    let server = MockServer::start().await;
    let text = reply(&[(Some(1), [0.5; 4]), (Some(2), [0.0; 4])]).to_string();
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(answer(&text))
        .mount(&server)
        .await;
    let (hypotheses, _) = generate::hypothesize(&client(&server), "m", &common::case(), 0)
        .await
        .unwrap();
    assert_eq!(hypotheses.len(), 2);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let err = generate::hypothesize(&client(&server), "m", &common::case(), 0)
        .await
        .unwrap_err();
    assert!(matches!(
        err.downcast_ref(),
        Some(OpenAiError::Unauthorized)
    ));
}
