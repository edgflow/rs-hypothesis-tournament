# hypothesis-tournament

An AI hypothesis tournament, in Rust.

A test fails on CI about one run in four. Three teammates have three
explanations, and an AI is happy to tell you which one it believes. This
tool doesn't care what anyone believes. Every explanation becomes a **bet**:
for each condition, how often will the test fail? Then Rust runs small
experiments, scores each bet against what actually happened, and ranks the
explanations by evidence.

```text
question + explanations  →  AI writes bets  →  Rust picks an experiment  →  runs it  →  scores every bet  →  ranking
```

- **AI: hypothesis generation.** The model turns each explanation into bets on
  every condition, and adds a hypothesis of its own. It also says how
  confident it is. That number is shown, never scored.
- **Rust: experiment runner.** Each round runs the condition where the
  hypotheses still in the game disagree most (expected information, in bits),
  20 runs per experiment. A hypothesis is out once it is 100× less likely than
  the leader.
- **A baseline that must be beaten.** `H0 just flaky` says none of the knobs
  matter and bets whatever single fail rate fits the evidence best. To win, an
  explanation has to show that a knob matters: the test fails where it said
  and passes where it said.

## Try it without an API key

`examples/checkout/` is a synthetic case: the failing test, the facts, three
explanations and three knobs (`threads` 1|8, `tz` UTC|Pacific/Auckland,
`clock` 09:00|21:00). `hypotheses.json` is a real answer from `gpt-5.4-mini`,
and `truth.json` is the hidden cause the simulated runs are drawn from. The
model never sees it.

```bash
cargo run -- show examples/checkout/case.json examples/checkout/hypotheses.json
cargo run -- run  examples/checkout/case.json examples/checkout/hypotheses.json --truth examples/checkout/truth.json
```

```text
round 1 threads=1 tz=UTC clock=21:00  · 0.72 bits per run
        ✗✗✗✗✗✗✗✗✗✗✗✗✗✗✗✗✗✗✗✗  20/20 failed
        H0 33%  H1 33%  H3 33%  out: H2 H4

round 2 threads=1 tz=UTC clock=09:00  · 0.78 bits per run
        ✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓  0/20 failed
        H1 >99%  out: H0 H2 H3 H4

winner  H1 Time zone: 10^11.7× more likely than the runner-up

      hypothesis            evidence   the model's confidence
  1.  H1  Time zone              >99%      15%
  2.  H0  just flaky              <1%        –
  3.  H3  Float rounding          <1%       2%
  4.  H2  Global race             <1%      65%
  5.  H4  Night cleanup           <1%      18%
```

The model's favourite (the race, 65%) is out after the first experiment: the
test failed 20 times out of 20 with a single thread.

## Your own case

```bash
cp .env.example .env    # add your OPENAI_API_KEY
cargo run -- hypothesize my/case.json --extra 1 --out runs/hypotheses.json
cargo run -- show my/case.json runs/hypotheses.json      # edit the bets if you disagree
cargo run -- run my/case.json runs/hypotheses.json \
  --cmd 'RUST_TEST_THREADS=$THREADS faketime "$CLOCK" cargo test -q promo_code'
```

| command | what it does | calls the model |
|---|---|---|
| `hypothesize` | one request: every explanation (plus `--extra` of the model's own) becomes a bet on every condition, checked against a strict JSON schema and validated again in Rust | yes |
| `show` | the bets as a table: one row per condition, one column per hypothesis | no |
| `run --truth` | the tournament on synthetic evidence: hidden fail rates and a seeded generator (`--seed`), so every run is reproducible | no |
| `run --cmd` | the tournament on real evidence: the command runs once per trial with every knob as an environment variable (`THREADS=8 TZ=UTC CLOCK=21:00`); exit code 0 means the test passed | no |

Options: `--runs` per experiment (20), `--max-rounds` (8), `--margin` (100: how
many times less likely than the leader a hypothesis must be to be out).

A tournament ends in one of three ways: a **winner** (every rival is out), a
**tie** (no condition separates the leaders: their bets agree everywhere, so
add a knob that does), or **undecided** (out of rounds).

## How it works

| file | what it holds |
|---|---|
| `src/case.rs` | the case file, the knobs, every condition (all combinations of knob values) |
| `src/hypothesis.rs` | a hypothesis = a bet per condition; the `just flaky` baseline; validation |
| `src/generate.rs` | the prompt, the strict JSON schema, and the checks on the model's answer |
| `src/openai.rs` | `POST /responses` with structured output, retries, no `store` |
| `src/world.rs` | `World`: `Simulated` (synthetic, seeded SplitMix64) and `Command` (a real test) |
| `src/score.rs` | log-likelihood of a bet (clamped to 2%..98%), shares, expected information |
| `src/tournament.rs` | pick the next experiment, run it, score, eliminate, decide |

`cargo test` runs offline: the OpenAI API is a local stand-in (`wiremock`).

## License

MIT
