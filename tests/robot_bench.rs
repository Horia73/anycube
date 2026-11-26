//! What the robot's planner and choosing by robot time are worth, and what
//! they cost, on random 3x3x3 states: the simulated MultiCuber time of the
//! scan and of the chosen solution, against the 2018 scan and the 2018 plan
//! of the plain solver's answer (what /api/solve gives), and the compute
//! time of the choice with the tables loaded.
//!
//!     cargo test --release --test robot_bench -- --ignored --nocapture
//!
//! ROBOT_BENCH_CUBES (default 100) and ROBOT_BENCH_SEARCH_MS (a list, default "15") change it;
//! ROBOT_BENCH_SLACK, ROBOT_BENCH_PHASE2 and ROBOT_BENCH_DEPTH the search.
#![cfg(feature = "robots")]

use anycube::api::{check, random_state, solve, Input, Mode};
use anycube::robot::fastest::{self, Options};
use anycube::robot::multicuber::legacy::{BRule, Legacy};
use anycube::robot::multicuber::optimize::Space;
use anycube::robot::multicuber::planner::{Lift, Planner, RobotState};
use anycube::robot::multicuber::timing::TimeModel;
use anycube::robot::Turn;
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
    for n in [3usize, 4, 5] {
        let model = TimeModel::for_size(n);
        let t = Instant::now();
        let reps = 20;
        for _ in 0..reps {
            std::hint::black_box(Space::new(n, &model));
        }
        eprintln!("the quickest ways between the robot's states, {0}x{0}x{0}: made in {1:.2} ms", n, t.elapsed().as_secs_f64() * 1000.0 / reps as f64);
    }
    let space = Space::new(3, &timing);
    // The scan, from the parked robot (lift down, flipper up).
    let parked = RobotState { lift: Lift::Level(0), flip_up: true };
    let mut l = Legacy::new(Planner::new(3).with_timing(&timing).from_state(parked), BRule::Legacy);
    l.scan(false);
    let scan_2018 = l.p.ms;
    let scan_ms = |seated: bool| {
        let t = Instant::now();
        let mut p = Planner::new(3).with_timing(&timing).from_state(parked);
        space.scan(&mut p, seated);
        (p.ms, t.elapsed().as_secs_f64() * 1000.0)
    };
    let (scan_seated, scan_compute) = scan_ms(true);
    let (scan_free, _) = scan_ms(false);
    eprintln!("scan, robot ms (motion only): 2018 {:.0}, planner {:.0} (pictures after a tip, the default; planned in {:.2} ms), {:.0} (pictures anywhere)", scan_2018, scan_seated, scan_compute, scan_free);
    // Where a scan leaves the robot.
    let after_scan = RobotState { lift: Lift::Level(0), flip_up: false };
    let cost = |m: &[Turn]| space.cost(after_scan, m);
    let legacy = |m: &[Turn]| {
        let mut l = Legacy::new(Planner::costing(3, &timing).from_state(after_scan), BRule::Legacy);
        l.solution(m);
        l.p.ms
    };
    let states: Vec<_> = (0..cubes as u64).map(|k| check(3, Input::Moves(&random_state(3, 1000 + k).unwrap()), 3).unwrap()).collect();
    // Warm up the threads and caches once.
    let _ = fastest::choose(&states[0], &cost, &Options::default());
    let (mut plain_ms, mut plain_opt_ms, mut plain_len, mut plain_compute) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut cost_us = Vec::new();
    for c in &states {
        let t = Instant::now();
        let s = solve(c, Mode::Normal).unwrap();
        plain_compute.push(t.elapsed().as_secs_f64() * 1000.0);
        let turns: Vec<Turn> = s.moves.iter().map(|&m| Turn::from_index(m)).collect();
        plain_ms.push(legacy(&turns));
        let t = Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(cost(std::hint::black_box(&turns)));
        }
        cost_us.push(t.elapsed().as_secs_f64() * 1000.0);
        plain_opt_ms.push(cost(&turns));
        plain_len.push(turns.len() as f64);
    }
    eprintln!("{} random 3x3x3 states; robot ms from a scanned cube (lift down, flipper down), MEDIUM preset model", cubes);
    eprintln!("plain solver, 2018 plan          {}", stats(&plain_ms));
    eprintln!("plain solver, the planner        {}", stats(&plain_opt_ms));
    eprintln!("plain solver, moves              {}", stats(&plain_len));
    eprintln!("plain solver, compute ms         {}", stats(&plain_compute));
    eprintln!("one solution's robot time, us    {}", stats(&cost_us));
    for &budget in &budgets {
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
        let mut opts = Options { search_ms: budget, ..Options::default() };
        opts.slack = env("ROBOT_BENCH_SLACK").unwrap_or(opts.slack);
        opts.phase2 = env("ROBOT_BENCH_PHASE2").unwrap_or(opts.phase2);
        opts.depth = env("ROBOT_BENCH_DEPTH").unwrap_or(opts.depth);
        opts.total_ms = env("ROBOT_BENCH_TOTAL_MS").map_or(opts.total_ms + budget - 15.0, |v| v as f64);
        let (mut est, mut compute, mut cands, mut sols, mut gain, mut gain_opt, mut len, mut whole, mut whole_2018, mut plan_ms) =
            (vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
        for (k, c) in states.iter().enumerate() {
            let ch = fastest::choose(c, &cost, &opts).unwrap();
            let t = Instant::now();
            let mut p = Planner::new(3).with_timing(&timing).from_state(after_scan);
            space.solution(&mut p, &ch.moves);
            plan_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            assert!((p.ms - ch.est_ms).abs() < 1.0);
            est.push(ch.est_ms);
            compute.push(ch.compute_ms);
            cands.push(ch.candidates as f64);
            sols.push(ch.solutions as f64);
            len.push(ch.moves.len() as f64);
            gain.push(100.0 * (plain_ms[k] - ch.est_ms) / plain_ms[k]);
            gain_opt.push(100.0 * (plain_opt_ms[k] - ch.est_ms) / plain_opt_ms[k]);
            whole.push(scan_seated + ch.est_ms);
            whole_2018.push(scan_2018 + plain_ms[k]);
        }
        eprintln!("--- search {} ms, slack {}, phase2 {}, depth {}", budget, opts.slack, opts.phase2, opts.depth);
        eprintln!("chosen, solve alone, robot ms    {}", stats(&est));
        eprintln!("saved vs plain + 2018 plan, %    {}", stats(&gain));
        eprintln!("saved vs plain + the planner, %  {}", stats(&gain_opt));
        eprintln!("scan + solve, robot ms           {}", stats(&whole));
        eprintln!("scan + solve as in 2018, ms      {}", stats(&whole_2018));
        eprintln!("compute ms (choice)              {}", stats(&compute));
        eprintln!("compute ms (the chosen's plan)   {}", stats(&plan_ms));
        eprintln!("candidates (with orders)         {}", stats(&cands));
        eprintln!("solutions from the search        {}", stats(&sols));
        eprintln!("chosen moves                     {}", stats(&len));
    }
}
