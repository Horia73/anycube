//! What choosing by robot time is worth, and what it costs, on random
//! 3x3x3 states: the simulated MultiCuber time of the chosen plan against
//! the legacy plan of the plain solver's answer (what /api/solve gives),
//! and the compute time of the choice with the tables loaded.
//!
//!     cargo test --release --test robot_bench -- --ignored --nocapture
//!
//! ROBOT_BENCH_CUBES (default 100) and ROBOT_BENCH_SEARCH_MS (a list, default "15") change it;
//! ROBOT_BENCH_SLACK, ROBOT_BENCH_PHASE2 and ROBOT_BENCH_DEPTH the search, ROBOT_BENCH_LEGACY_ONLY
//! leaves out the optimizing planner.
#![cfg(feature = "robots")]

use anycube::api::{check, random_state, solve, Input, Mode};
use anycube::robot::fastest::{self, Options};
use anycube::robot::multicuber::optimize;
use anycube::robot::multicuber::planner::{BRule, Lift, Planner, RobotState};
use anycube::robot::multicuber::timing::TimeModel;
use anycube::robot::{PlanMode, Turn};
use std::time::Instant;

fn stats(v: &[f64]) -> String {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    format!("mean {:8.1}  min {:8.1}  median {:8.1}  p90 {:8.1}  max {:8.1}", mean, s[0], s[s.len() / 2], s[s.len() * 9 / 10], s[s.len() - 1])
}

#[test]
#[ignore]
fn robot_time_against_plain_solutions() {
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
    let cubes: usize = std::env::var("ROBOT_BENCH_CUBES").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let budgets: Vec<f64> = std::env::var("ROBOT_BENCH_SEARCH_MS")
        .unwrap_or_else(|_| "15".into())
        .split(',')
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    let t = Instant::now();
    anycube::api::solve::load(3);
    eprintln!("tables loaded in {} ms", t.elapsed().as_millis());
    let timing = TimeModel::for_size(3);
    // Where a scan leaves the robot.
    let after_scan = RobotState { lift: Lift::Level(0), flip_up: false };
    let cost = |m: &[Turn], mode: PlanMode| {
        let mut p = Planner::costing(3, BRule::Legacy, &timing).from_state(after_scan);
        match mode {
            PlanMode::Legacy => p.solution(m),
            PlanMode::Optimized => {
                let mut q = Planner::new(3, BRule::Legacy).with_timing(&timing).from_state(after_scan);
                optimize::solution(&mut q, m, &timing);
                return q.ms;
            }
        }
        p.ms
    };
    let states: Vec<_> = (0..cubes as u64).map(|k| check(3, Input::Moves(&random_state(3, 1000 + k).unwrap()), 3).unwrap()).collect();
    // Warm up the threads and caches once.
    let _ = fastest::choose(&states[0], &cost, &Options::default());
    let mut plain_ms = Vec::new();
    let mut plain_opt_ms = Vec::new();
    let mut plain_len = Vec::new();
    let mut plain_compute = Vec::new();
    for c in &states {
        let t = Instant::now();
        let s = solve(c, Mode::Normal).unwrap();
        plain_compute.push(t.elapsed().as_secs_f64() * 1000.0);
        let turns: Vec<Turn> = s.moves.iter().map(|&m| Turn::from_index(m)).collect();
        plain_ms.push(cost(&turns, PlanMode::Legacy));
        plain_opt_ms.push(cost(&turns, PlanMode::Optimized));
        plain_len.push(turns.len() as f64);
    }
    eprintln!("{} random 3x3x3 states; robot ms from a scanned cube (lift down, flipper down), MEDIUM preset model", cubes);
    eprintln!("plain solver, legacy plan        {}", stats(&plain_ms));
    eprintln!("plain solver, optimizing plan    {}", stats(&plain_opt_ms));
    eprintln!("plain solver, moves              {}", stats(&plain_len));
    eprintln!("plain solver, compute ms         {}", stats(&plain_compute));
    for &budget in &budgets {
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
        let plans = if std::env::var_os("ROBOT_BENCH_LEGACY_ONLY").is_some() { vec![PlanMode::Legacy] } else { vec![PlanMode::Legacy, PlanMode::Optimized] };
        for plan in plans {
            let mut opts = Options { search_ms: budget, plan, ..Options::default() };
            opts.slack = env("ROBOT_BENCH_SLACK").unwrap_or(opts.slack);
            opts.phase2 = env("ROBOT_BENCH_PHASE2").unwrap_or(opts.phase2);
            opts.depth = env("ROBOT_BENCH_DEPTH").unwrap_or(opts.depth);
            opts.total_ms = env("ROBOT_BENCH_TOTAL_MS").map_or(opts.total_ms + budget - 15.0, |v| v as f64);
            let (mut est, mut compute, mut cands, mut sols, mut gain, mut len) = (vec![], vec![], vec![], vec![], vec![], vec![]);
            for (k, c) in states.iter().enumerate() {
                let ch = fastest::choose(c, &cost, &opts).unwrap();
                est.push(ch.est_ms);
                compute.push(ch.compute_ms);
                cands.push(ch.candidates as f64);
                sols.push(ch.solutions as f64);
                len.push(ch.moves.len() as f64);
                gain.push(100.0 * (plain_ms[k] - ch.est_ms) / plain_ms[k]);
            }
            eprintln!("--- search {} ms, {:?} plan, slack {}, phase2 {}, depth {}", budget, plan, opts.slack, opts.phase2, opts.depth);
            eprintln!("chosen robot ms                  {}", stats(&est));
            eprintln!("saved vs plain legacy, %         {}", stats(&gain));
            eprintln!("compute ms                       {}", stats(&compute));
            eprintln!("candidates (with orders)         {}", stats(&cands));
            eprintln!("solutions from the search        {}", stats(&sols));
            eprintln!("chosen moves                     {}", stats(&len));
        }
    }
}
