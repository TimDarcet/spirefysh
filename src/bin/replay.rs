use std::{env, process::ExitCode};
use sts2_sim::{lockstep_trace, replay_trace};

fn main() -> ExitCode {
    if env::args().nth(1).as_deref() == Some("--lockstep") {
        return lockstep(env::args().nth(2));
    }
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: cargo run --bin replay -- [--lockstep] TRACE.ndjson [LIMIT]");
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

/// Plays the trace's run from its seed and reports the first decision where it leaves the game.
fn lockstep(path: Option<String>) -> ExitCode {
    let Some(path) = path else {
        eprintln!("usage: cargo run --bin replay -- --lockstep TRACE.ndjson");
        return ExitCode::FAILURE;
    };
    let report = match lockstep_trace(&path) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "{} of {} decisions in sync, floor {}",
        report.matched, report.decisions, report.floor
    );
    for (sequence, drift) in &report.rng_drifts {
        println!("RNG drift from sequence {sequence}: {drift}");
    }
    let Some(divergence) = report.divergence else {
        return ExitCode::SUCCESS;
    };
    println!(
        "diverged at sequence {} ({})",
        divergence.sequence, divergence.action
    );
    for difference in &divergence.differences {
        println!("  {difference}");
    }
    ExitCode::FAILURE
}
