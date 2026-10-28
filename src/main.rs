use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use hypothesis_tournament::case::Case;
use hypothesis_tournament::config::Config;
use hypothesis_tournament::generate;
use hypothesis_tournament::hypothesis::{self, Hypothesis, HypothesisFile, Origin};
use hypothesis_tournament::openai::OpenAi;
use hypothesis_tournament::style::{bold, cyan, dim, green, red, yellow};
use hypothesis_tournament::tournament::{self, Report, Round, Rules, Verdict};
use hypothesis_tournament::world::{self, World};

#[derive(Parser)]
#[command(
    version,
    about = "Competing explanations for a failing test, ranked by evidence, not by confidence"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Ask the model to turn the case's explanations into hypotheses that bet on every condition.
    Hypothesize {
        case: PathBuf,
        /// Where to write the hypotheses (JSON).
        #[arg(long)]
        out: PathBuf,
        /// How many hypotheses of its own the model adds.
        #[arg(long, default_value_t = 0)]
        extra: usize,
    },
    /// Show every bet: one row per condition, one column per hypothesis.
    Show { case: PathBuf, hypotheses: PathBuf },
    /// Run the tournament: experiments until one hypothesis is left standing.
    Run {
        case: PathBuf,
        hypotheses: PathBuf,
        /// Synthetic evidence: the hidden fail rate per condition (JSON).
        #[arg(long, conflicts_with = "cmd", required_unless_present = "cmd")]
        truth: Option<PathBuf>,
        /// Seed for the synthetic evidence.
        #[arg(long, default_value_t = 7)]
        seed: u64,
        /// Real evidence: a shell command that runs the test once, with every knob as an
        /// environment variable (THREADS=8 TZ=UTC ...). Exit code 0 means it passed.
        #[arg(long)]
        cmd: Option<String>,
        /// Runs per experiment.
        #[arg(long, default_value_t = 20)]
        runs: u32,
        #[arg(long, default_value_t = 8)]
        max_rounds: u32,
        /// A hypothesis is out once it is this many times less likely than the leader.
        #[arg(long, default_value_t = 100.0)]
        margin: f64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    match Cli::parse().command {
        Command::Hypothesize { case, out, extra } => {
            let case = Case::load(&case)?;
            let config = Config::from_env()?;
            println!(
                "asking {}: {} explanations + {extra} of its own, {} conditions each",
                config.model,
                case.explanations.len(),
                case.conditions().len()
            );
            let client = OpenAi::new(&config.openai_api_key, &config.openai_base_url);
            let (hypotheses, usage) =
                generate::hypothesize(&client, &config.model, &case, extra).await?;
            for h in &hypotheses {
                let origin = match h.origin {
                    Origin::Explanation(n) => format!("explanation {n}"),
                    _ => "the model's own".into(),
                };
                let confidence = h.confidence.map_or(String::new(), |c| {
                    format!("confidence {}", bold(percent(c)))
                });
                println!(
                    "{} {} {origin:17} {confidence}",
                    cyan(&h.id),
                    bold(format!("{:18}", h.name))
                );
                println!("   {}", dim(&h.claim));
            }
            let file = HypothesisFile {
                model: config.model,
                hypotheses,
            };
            hypothesis::save(&out, &file)?;
            println!(
                "wrote {} ({} bets, {} + {} tokens)",
                out.display(),
                file.hypotheses.len() * case.conditions().len(),
                usage.input_tokens,
                usage.output_tokens
            );
        }
        Command::Show { case, hypotheses } => {
            let case = Case::load(&case)?;
            let field = field(&case, &hypotheses)?;
            print_legend(&field);
            let width = case
                .conditions()
                .iter()
                .map(|c| c.key().len())
                .max()
                .unwrap_or(0);
            print!("\n{}", dim(format!("{:width$}", "fails in % of runs")));
            for h in &field {
                print!("{}", cyan(format!("{:>6}", h.id)));
            }
            println!();
            for condition in case.conditions() {
                print!("{:width$}", condition.key());
                for h in &field {
                    let rate = h.bet(&condition);
                    let cell = format!("{:>6}", percent(rate));
                    print!("{}", if rate == 0.0 { dim(cell) } else { cell });
                }
                println!();
            }
            print!("{}", yellow(format!("{:width$}", "the model's confidence")));
            for h in &field {
                print!("{:>6}", h.confidence.map_or("–".into(), percent));
            }
            println!();
        }
        Command::Run {
            case,
            hypotheses,
            truth,
            seed,
            cmd,
            runs,
            max_rounds,
            margin,
        } => {
            let case_path = case;
            let case = Case::load(&case_path)?;
            let field = field(&case, &hypotheses)?;
            let mut world: Box<dyn World> = match (truth, cmd) {
                (Some(truth), _) => Box::new(world::Simulated::load(&truth, &case, seed)?),
                (None, Some(cmd)) => {
                    let dir = case_path.parent().unwrap_or(Path::new("."));
                    Box::new(world::Command::new(cmd, dir))
                }
                (None, None) => bail!("give --truth or --cmd"),
            };
            if runs == 0 || margin <= 1.0 {
                bail!("--runs must be above 0 and --margin above 1");
            }
            print_legend(&field);
            println!(
                "{}  {} · {runs} runs per experiment\n",
                dim("evidence"),
                world.describe()
            );
            let rules = Rules {
                runs,
                max_rounds,
                margin,
            };
            let report = tournament::run(&case, &field, world.as_mut(), rules, |round| {
                print_round(round, &field)
            })?;
            print_verdict(&report, &field);
        }
    }
    Ok(())
}

/// The hypotheses from the file, after the baseline every one of them must beat.
fn field(case: &Case, path: &Path) -> Result<Vec<Hypothesis>> {
    let file = hypothesis::load(path, case)?;
    if file.hypotheses.iter().any(|h| h.id == "H0") {
        bail!("H0 is reserved for the baseline ({})", path.display());
    }
    let mut field = vec![Hypothesis::just_flaky(case)];
    field.extend(file.hypotheses);
    Ok(field)
}

fn print_legend(field: &[Hypothesis]) {
    let names: Vec<String> = field
        .iter()
        .map(|h| {
            let name = if h.origin == Origin::Model {
                yellow(&h.name)
            } else {
                h.name.clone()
            };
            format!("{} {name}", cyan(&h.id))
        })
        .collect();
    println!("{}  {}", dim("suspects"), names.join(" · "));
}

fn print_round(round: &Round, field: &[Hypothesis]) {
    println!(
        "{} {}  {}",
        bold(format!("round {}", round.number)),
        round.condition,
        dim(format!("· {:.2} bits per run", round.bits))
    );
    let runs: String = round
        .results
        .iter()
        .map(|&failed| if failed { red("✗") } else { green("✓") })
        .collect();
    println!(
        "        {runs}  {}/{} failed",
        round.failed(),
        round.results.len()
    );
    let mut order: Vec<usize> = (0..field.len()).filter(|&i| !round.out[i]).collect();
    order.sort_by(|&a, &b| round.shares[b].total_cmp(&round.shares[a]));
    let alive: Vec<String> = order
        .iter()
        .map(|&i| format!("{} {}", cyan(&field[i].id), percent(round.shares[i])))
        .collect();
    let out: Vec<&str> = (0..field.len())
        .filter(|&i| round.out[i])
        .map(|i| field[i].id.as_str())
        .collect();
    let out = if out.is_empty() {
        String::new()
    } else {
        dim(format!("  out: {}", out.join(" ")))
    };
    println!("        {}{out}\n", alive.join("  "));
}

fn print_verdict(report: &Report, field: &[Hypothesis]) {
    let h = |i: usize| format!("{} {}", cyan(&field[i].id), bold(&field[i].name));
    match &report.verdict {
        Verdict::Winner { leader, log10_odds } => println!(
            "{}  {}: {} more likely than the runner-up",
            green("winner"),
            h(*leader),
            odds(*log10_odds)
        ),
        Verdict::Tie { leaders } => {
            let names: Vec<String> = leaders.iter().map(|&i| h(i)).collect();
            println!(
                "{}  {}: no condition separates them; add a knob that does",
                yellow("tie"),
                names.join(" and ")
            )
        }
        Verdict::Undecided { leader, log10_odds } => println!(
            "{}  {} leads, {} more likely than the runner-up; out of rounds",
            yellow("undecided"),
            h(*leader),
            odds(*log10_odds)
        ),
    }
    let shares = hypothesis_tournament::score::shares(&report.log_likelihoods);
    let mut order: Vec<usize> = (0..field.len()).collect();
    order.sort_by(|&a, &b| shares[b].total_cmp(&shares[a]));
    println!(
        "\n{}",
        dim("      hypothesis            evidence   the model's confidence")
    );
    for (rank, &i) in order.iter().enumerate() {
        let confidence = field[i].confidence.map_or("–".into(), percent);
        println!(
            "  {}.  {} {:18} {:>8}   {:>6}",
            rank + 1,
            cyan(format!("{:3}", field[i].id)),
            field[i].name,
            percent(shares[i]),
            confidence
        );
    }
    let runs: usize = report.rounds.iter().map(|r| r.results.len()).sum();
    println!(
        "\n{}",
        dim(format!(
            "{} experiments, {runs} runs. Confidence is shown, never scored.",
            report.rounds.len()
        ))
    );
}

/// `0.3` → `30%`; extremes keep a hint that they are not exact.
fn percent(share: f64) -> String {
    if share > 0.0 && share < 0.005 {
        "<1%".into()
    } else if share < 1.0 && share > 0.995 {
        ">99%".into()
    } else {
        format!("{:.0}%", share * 100.0)
    }
}

/// `2.0` → `100×`; past a million, powers of ten.
fn odds(log10_odds: f64) -> String {
    if log10_odds < 6.0 {
        let n = 10f64.powf(log10_odds).round() as u64;
        let digits = n.to_string();
        let mut grouped = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        format!("{grouped}×")
    } else {
        format!("10^{log10_odds:.1}×")
    }
}
