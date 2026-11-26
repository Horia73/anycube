//! What the robot's planner and choosing by robot time are worth, and what
//! they cost, on random 3x3x3 states: the simulated MultiCuber time of the
//! scan and of the chosen solution, against the 2018 scan and the 2018 plan
//! of the plain solver's answer (what /api/solve gives), and the compute
//! time of the choice with the tables loaded.
//!
//!     cargo test --release --test robot_bench -- --ignored --nocapture
//!
//! ROBOT_BENCH_CUBES (default 100) and ROBOT_BENCH_SEARCH_MS (a list; default: the choice's own) change it;
//! ROBOT_BENCH_SLACK, ROBOT_BENCH_PHASE2 and ROBOT_BENCH_DEPTH the search.
//! ROBOT_BENCH_VARIANTS="normal:150:15,fast:0:15" tries several choices, each the plain solver's
//! mode, its time and the time of the search for more; ROBOT_BENCH_TABLE=1 prints, state by
//! state, the robot time of the shortest solution (the plain solver's) and of the chosen one.
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
        .unwrap_or_else(|_| Options::default().search_ms.to_string())
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
    // Each variant: the plain solver's mode, its time (ms), the time of the search for more (ms).
    let variants = std::env::var("ROBOT_BENCH_VARIANTS").unwrap_or_else(|_| {
        let d = Options::default();
        budgets.iter().map(|b| format!("{}:{}:{}", if d.mode == Mode::Fast { "fast" } else { "normal" }, d.plain_ms, b)).collect::<Vec<_>>().join(",")
    });
    let table = std::env::var_os("ROBOT_BENCH_TABLE").is_some();
    for variant in variants.split(',') {
        let parts: Vec<&str> = variant.split(':').collect();
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok());
        let mut opts = Options::default();
        opts.mode = if parts[0] == "fast" { Mode::Fast } else { Mode::Normal };
        opts.plain_ms = parts[1].parse().unwrap();
        opts.search_ms = parts[2].parse().unwrap();
        opts.slack = env("ROBOT_BENCH_SLACK").unwrap_or(opts.slack);
        opts.phase2 = env("ROBOT_BENCH_PHASE2").unwrap_or(opts.phase2);
        opts.depth = env("ROBOT_BENCH_DEPTH").unwrap_or(opts.depth);
        opts.total_ms = env("ROBOT_BENCH_TOTAL_MS").map_or(opts.search_ms + 10.0, |v| v as f64);
        let (mut est, mut compute, mut cands, mut sols, mut gain, mut gain_opt, mut len, mut whole, mut whole_2018, mut plan_ms) =
            (vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
        let (mut own, mut own_len, mut own_compute, mut complete, mut slower, mut longer_wins) = (vec![], vec![], vec![], 0, 0, 0);
        if table {
            eprintln!("state | shortest (plain solver, normal, no time limit): moves, robot ms | chosen: moves, robot ms | chosen - shortest, ms");
        }
        for (k, c) in states.iter().enumerate() {
            let ch = fastest::choose(c, &cost, &opts).unwrap();
            let t = Instant::now();
            let mut p = Planner::new(3).with_timing(&timing).from_state(after_scan);
            space.solution(&mut p, &ch.moves);
            plan_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            assert!((p.ms - ch.est_ms).abs() < 1.0);
            // Never slower than the plain answer it was given.
            assert!(ch.est_ms <= ch.plain.est_ms);
            if ch.plain.complete && opts.mode == Mode::Normal {
                assert!((ch.plain.est_ms - plain_opt_ms[k]).abs() < 1e-6, "a plain search that ran to its end gives /api/solve's answer");
            }
            if table {
                eprintln!(
                    "{:5} | {:2} {:8.0} | {:2} {:8.0} | {:+8.0}{}",
                    k,
                    plain_len[k],
                    plain_opt_ms[k],
                    ch.moves.len(),
                    ch.est_ms,
                    ch.est_ms - plain_opt_ms[k],
                    if ch.plain.complete { "" } else { "  (the plain search was cut short)" }
                );
            }
            complete += ch.plain.complete as usize;
            slower += (ch.est_ms > plain_opt_ms[k] + 1e-6) as usize;
            longer_wins += (ch.moves.len() as f64 > plain_len[k] && ch.est_ms < plain_opt_ms[k] - 1e-6) as usize;
            own.push(ch.plain.est_ms);
            own_len.push(ch.plain.moves.len() as f64);
            own_compute.push(ch.plain.compute_ms);
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
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        eprintln!("--- plain {} within {} ms, then search {} ms, slack {}, phase2 {}, depth {}", parts[0], opts.plain_ms, opts.search_ms, opts.slack, opts.phase2, opts.depth);
        eprintln!("its plain answer, robot ms       {}", stats(&own));
        eprintln!("its plain answer, moves          {}", stats(&own_len));
        eprintln!("its plain answer, compute ms     {}", stats(&own_compute));
        eprintln!("plain searches that ran to their end: {} of {}", complete, cubes);
        eprintln!("chosen, solve alone, robot ms    {}", stats(&est));
        eprintln!("chosen moves                     {}", stats(&len));
        eprintln!("saved vs plain + 2018 plan, %    {}", stats(&gain));
        eprintln!("saved vs shortest + planner, %   {}", stats(&gain_opt));
        eprintln!("chosen slower than the shortest (normal, no limit): {} of {}; chosen longer than it and quicker: {}", slower, cubes, longer_wins);
        eprintln!("scan + solve, robot ms           {}", stats(&whole));
        eprintln!("scan + solve as in 2018, ms      {}", stats(&whole_2018));
        eprintln!("compute ms (choice)              {}", stats(&compute));
        eprintln!("compute ms (the chosen's plan)   {}", stats(&plan_ms));
        eprintln!("candidates (with orders)         {}", stats(&cands));
        eprintln!("solutions from the searches      {}", stats(&sols));
        eprintln!("compute + robot, mean ms         {:8.1}   (the shortest alone: {:8.1} + its compute {:.1})", mean(&compute) + mean(&est), mean(&plain_opt_ms), mean(&plain_compute));
    }
    // The overlap study (robots-spec/OVERLAP.md): the flipper's empty moves made while the lift
    // or the rotator moves. The same states, the choice with its defaults.
    let over = Space::with_overlap(3, &timing);
    let cost_over = |m: &[Turn]| over.cost(after_scan, m);
    let scan_over = {
        let mut p = Planner::new(3).with_timing(&timing).from_state(parked);
        over.scan(&mut p, true);
        p.ms
    };
    // Of a plan: the lift's moves, their time, the pairs started together.
    let lifts = |space: &Space, moves: &[Turn]| {
        let mut p = Planner::new(3).with_timing(&timing).from_state(after_scan);
        space.solution(&mut p, moves);
        let (spans, total) = anycube::robot::multicuber::overlap::timeline(&timing, &p.ops);
        assert!((total - p.ms).abs() < 1e-6);
        assert!(anycube::robot::multicuber::overlap::check(&timing, after_scan, &p.ops, &spans).is_empty());
        let lift: Vec<&anycube::robot::multicuber::planner::Op> = p.ops.iter().filter(|o| o.line().starts_with("LIFT")).collect();
        let of = |what: &str| p.ops.iter().filter(|o| o.line().starts_with(what)).map(|o| timing.op_ms(o)).sum::<f64>();
        let up = lift.iter().filter(|o| o.lift_before == Lift::Level(0)).count() as f64;
        ((lift.len() as f64, of("LIFT"), p.ops.iter().filter(|o| o.joined).count() as f64, p.ms), (of("ROT"), of("FLIP"), up))
    };
    let opts = Options::default();
    let (mut today, mut same, mut chosen, mut plain_today, mut plain_over) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut lift_n, mut lift_ms, mut lift_n_over, mut lift_ms_over, mut pairs) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut rot_ms, mut flip_ms, mut lifts_up) = (vec![], vec![], vec![]);
    for (k, c) in states.iter().enumerate() {
        let a = fastest::choose(c, &cost, &opts).unwrap();
        let b = fastest::choose(c, &cost_over, &opts).unwrap();
        today.push(a.est_ms);
        same.push(cost_over(&a.moves));
        chosen.push(b.est_ms);
        plain_today.push(plain_opt_ms[k]);
        plain_over.push(cost_over(&a.plain.moves).min(plain_opt_ms[k]));
        let ((n, ms, _, total), (rot, flip, up)) = lifts(&space, &a.moves);
        assert!((total - a.est_ms).abs() < 1.0);
        lift_n.push(n);
        lift_ms.push(ms);
        rot_ms.push(rot);
        flip_ms.push(flip);
        lifts_up.push(up);
        let ((n, ms, joined, total), _) = lifts(&over, &b.moves);
        assert!((total - b.est_ms).abs() < 1.0);
        lift_n_over.push(n);
        lift_ms_over.push(ms);
        pairs.push(joined);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    eprintln!("--- overlap study: the flipper's empty moves during lift moves and rotator turns (plain within {} ms, search {} ms)", opts.plain_ms, opts.search_ms);
    eprintln!("scan, robot ms: today {:.0}, with overlaps {:.0}", scan_seated, scan_over);
    eprintln!("today: chosen, robot ms          {}", stats(&today));
    eprintln!("the same solutions, overlapped   {}", stats(&same));
    eprintln!("chosen for overlaps, robot ms    {}", stats(&chosen));
    eprintln!("saved by overlaps: the same solutions {:.0} ms ({:.1} %), chosen anew {:.0} ms ({:.1} %)",
        mean(&today) - mean(&same), 100.0 * (1.0 - mean(&same) / mean(&today)), mean(&today) - mean(&chosen), 100.0 * (1.0 - mean(&chosen) / mean(&today)));
    eprintln!("pairs started together a solve   {}", stats(&pairs));
    eprintln!("lift moves a solve: today {:.1} taking {:.0} ms ({:.0} % of the solve), with overlaps {:.1} taking {:.0} ms",
        mean(&lift_n), mean(&lift_ms), 100.0 * mean(&lift_ms) / mean(&today), mean(&lift_n_over), mean(&lift_ms_over));
    eprintln!("today, a solve: the rotator {:.0} ms, the flipper {:.0} ms, {:.1} lift moves up from 0", mean(&rot_ms), mean(&flip_ms), mean(&lifts_up));
    let _ = (plain_today, plain_over);
}
