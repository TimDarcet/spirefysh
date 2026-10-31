use std::{env, process::ExitCode};
use sts2_sim::replay_trace;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: cargo run --bin replay -- TRACE.ndjson [LIMIT]");
        return ExitCode::FAILURE;
    };
    let limit = env::args()
        .nth(2)
        .and_then(|x| x.parse().ok())
        .unwrap_or(100);
    let report = match replay_trace(&path) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "{} decisions: {} replayed, {} unsupported, {} divergent, {} resyncs, {} inferred, {} \
         guessed, {} fallbacks, {} skipped",
        report.decisions,
        report.replayed,
        report.unsupported,
        report.divergences.len(),
        report.tallied("resync"),
        report.tallied("infer"),
        report.tallied("guess"),
        report.tallied("fallback"),
        report.tallied("skip"),
    );
    for divergence in report.divergences.iter().take(limit) {
        println!(
            "sequence {} action {} ({})",
            divergence.sequence,
            divergence
                .game_action_id
                .map(|x| x.to_string())
                .unwrap_or_else(|| "-".into()),
            divergence.action
        );
        for difference in &divergence.differences {
            println!("  {difference}");
        }
    }
    if report.divergences.len() > limit {
        println!("... {} more divergences", report.divergences.len() - limit);
    }
    if env::var_os("REPLAY_TALLY").is_some() {
        for (kind, count) in &report.tally {
            println!("tally {kind} {count}");
        }
    }
    if report.divergences.is_empty() && report.unsupported == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
