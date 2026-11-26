//! A solve never fails on a cube that can be solved, however little time the
//! choice of its solution is given: with no time at all, with a few
//! microseconds, with room for one solution only, every cube still gets a
//! solution that solves it and a plan that is safe on the sticker model.
//! (The search for candidates used to end with none when its time had run
//! out before its first solution, as on a program just started, whose
//! tables were not yet in memory, and the next 64 first phases it met had
//! no second phase within 12 moves: `solver_failed`, "no solution was
//! found", on a good cube, for 1 to 3 cubes in 1000 once the time was
//! gone. Hence the thousands of cubes here: ROBOT_STARVED_CUBES, default
//! 4000.)
//!
//! The plain solver's answer is always among the candidates, so the chosen
//! solution is never slower on the robot than it: held here for the answer
//! /api/solve gives, in both of its modes.
//!
//! (That the time is kept from the first choice on: tests/robot_warm.rs.)
//!
//! ROBOT_STARVED_SIZES (default "4") names the larger sizes tried; their
//! tables are gigabytes and are built when missing, as for tests/cli_solve.rs.
#![cfg(feature = "robots")]

use anycube::api::{check, Checked, Input, Mode};
use anycube::robot::fastest::{self, Options};
use anycube::robot::multicuber::optimize::Space;
use anycube::robot::multicuber::planner::{Lift, Planner, RobotState};
use anycube::robot::multicuber::timing::TimeModel;
use anycube::robot::physical::PhysicalCube;
use anycube::robot::Turn;

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn tables() {
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
}

fn scrambled(n: usize, moves: usize, rng: &mut Rng) -> Checked {
    let text: Vec<String> = (0..moves)
        .map(|_| {
            let depth = if n > 3 && rng.next(3) == 0 { 2 } else { 1 };
            Turn::Layers { face: rng.next(6) as u8, depth, turns: [1, -1, 2][rng.next(3)] }.name()
        })
        .collect();
    check(n, Input::Moves(&text.join(" ")), n).unwrap()
}

const AFTER_SCAN: RobotState = RobotState { lift: Lift::Level(0), flip_up: false };

/// The choice for a cube, held to solve it: by its moves, and by its plan on the sticker model.
fn chosen(c: &Checked, space: &Space, timing: &TimeModel, opts: &Options) -> fastest::Choice {
    let cost = |m: &[Turn]| space.cost(AFTER_SCAN, m);
    let choice = fastest::choose(c, &cost, opts).unwrap_or_else(|e| panic!("{}: {} with {:?}", e.code, e.message, opts));
    assert!(choice.candidates >= 1 && choice.solutions >= 1);
    assert!((cost(&choice.moves) - choice.est_ms).abs() < 1e-6);
    // The plain answer is a solution, costed by the same planner, and the chosen one is never slower.
    assert!((cost(&choice.plain.moves) - choice.plain.est_ms).abs() < 1e-6 && choice.est_ms <= choice.plain.est_ms);
    let mut plain = PhysicalCube::new(c.size);
    plain.facelets = c.facelets.clone();
    for t in &choice.plain.moves {
        let Turn::Layers { face, depth, turns } = *t else { panic!("the solver gives layer turns") };
        let power = match turns {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        plain.g.apply(&mut plain.facelets, (face as usize + 6 * (depth as usize - 1)) * 3 + power);
    }
    assert!(plain.is_solved(), "the plain answer {:?} does not solve the cube", choice.plain.moves);
    let mut p = Planner::new(c.size).with_timing(timing).from_state(AFTER_SCAN);
    space.solution(&mut p, &choice.moves);
    let mut cube = PhysicalCube::new(c.size);
    cube.facelets = c.facelets.clone();
    (cube.lift, cube.flip_up) = (AFTER_SCAN.lift, AFTER_SCAN.flip_up);
    for op in &p.ops {
        cube.apply_line(&op.line());
    }
    assert!(cube.is_solved(), "the plan of {:?} does not solve the cube", choice.names());
    assert!(cube.violations.is_empty(), "{:?}", cube.violations);
    choice
}

#[test]
fn a_starved_choice_still_solves_every_3x3x3() {
    tables();
    let timing = TimeModel::for_size(3);
    let space = Space::new(3, &timing);
    let mut rng = Rng(0x57a2f3d);
    // No time, next to none, a little; room for many solutions or for one.
    let budgets = [0.0, 0.0, 0.001, 0.02, 0.3, 2.0];
    let mut longest: usize = 0;
    let cubes: usize = std::env::var("ROBOT_STARVED_CUBES").ok().and_then(|v| v.parse().ok()).unwrap_or(4000);
    for k in 0..cubes {
        // Mostly well mixed; some a few moves from solved, where the first solution is the only short one.
        let c = scrambled(3, if k % 10 == 9 { 1 + rng.next(6) } else { 30 }, &mut rng);
        let search_ms = budgets[k % budgets.len()];
        // The plain search starved too (every other cube), or given a moment.
        let plain_ms = if k % 2 == 0 { 0.0 } else { 2.0 };
        let opts = Options { plain_ms, search_ms, total_ms: search_ms, max_solutions: if k % 4 == 3 { 1 } else { 2000 }, ..Options::default() };
        let choice = chosen(&c, &space, &timing, &opts);
        longest = longest.max(choice.moves.len());
    }
    // Starved, but still the two-phase search's own solutions.
    assert!(longest <= 26, "a solution of {} moves", longest);
}

/// The answer /api/solve gives is always among the candidates when the plain search is let run
/// to its end, so the chosen solution is never slower on the robot than it; and longer
/// solutions that are quicker do exist.
#[test]
fn the_chosen_solution_is_never_slower_than_the_plain_solvers() {
    tables();
    let timing = TimeModel::for_size(3);
    let space = Space::new(3, &timing);
    let cost = |m: &[Turn]| space.cost(AFTER_SCAN, m);
    let mut rng = Rng(0x18f21);
    let (mut longer_and_quicker, mut cubes) = (0, 0);
    for (mode, count) in [(Mode::Fast, 60), (Mode::Normal, 6)] {
        for _ in 0..count {
            let c = scrambled(3, 30, &mut rng);
            let opts = Options { plain_ms: 1e9, search_ms: 20.0, total_ms: 30.0, mode, ..Options::default() };
            let choice = chosen(&c, &space, &timing, &opts);
            let plain: Vec<Turn> = anycube::api::solve(&c, mode).unwrap().moves.iter().map(|&m| Turn::from_index(m)).collect();
            assert!(choice.plain.complete && choice.plain.moves == plain, "the plain answer is /api/solve's");
            assert!(choice.est_ms <= cost(&plain) + 1e-9);
            longer_and_quicker += (choice.moves.len() > plain.len() && choice.est_ms < cost(&plain)) as usize;
            cubes += 1;
        }
    }
    eprintln!("{} of {} chosen solutions are longer than the plain solver's and quicker on the robot", longer_and_quicker, cubes);
    assert!(longer_and_quicker > 0);
}

#[test]
fn a_starved_choice_still_solves_larger_cubes() {
    tables();
    let sizes: Vec<usize> = std::env::var("ROBOT_STARVED_SIZES").unwrap_or_else(|_| "4".into()).split(',').filter_map(|v| v.trim().parse().ok()).collect();
    let mut rng = Rng(0x4f5);
    for n in sizes {
        let timing = TimeModel::for_size(n);
        let space = Space::new(n, &timing);
        for k in 0..3 {
            let c = scrambled(n, if k == 2 { 3 } else { 40 }, &mut rng);
            // No time for a second orientation: the cube as it lies.
            let opts = Options { orientation_ms: 0.0, mode: Mode::Fast, ..Options::default() };
            let choice = chosen(&c, &space, &timing, &opts);
            assert_eq!(choice.solutions, 1);
        }
    }
}
