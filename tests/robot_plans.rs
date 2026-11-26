//! Robot plans on the sticker model: every plan, legacy or optimized, from
//! every starting state, leaves the cube solved, makes no step the legacy
//! planner never makes, and the optimized one is never slower. The time
//! model really decides which solution is taken.
#![cfg(feature = "robots")]

use anycube::api::{check, Input};
use anycube::cuben::Geometry;
use anycube::robot::fastest::{self, Options};
use anycube::robot::multicuber::optimize;
use anycube::robot::multicuber::planner::{BRule, Cmd, Lift, Planner, RobotState};
use anycube::robot::multicuber::timing::TimeModel;
use anycube::robot::physical::PhysicalCube;
use anycube::robot::{PlanMode, Turn};

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn random_turns(n: usize, count: usize, rng: &mut Rng) -> Vec<Turn> {
    (0..count)
        .map(|_| {
            let depth = if n > 3 && rng.next(10) < 4 { 1 + rng.next(n - 1) } else { 1 };
            Turn::Layers { face: rng.next(6) as u8, depth: depth as u8, turns: [1, -1, 2][rng.next(3)] }
        })
        .collect()
}

fn inverse(moves: &[Turn]) -> Vec<Turn> {
    moves
        .iter()
        .rev()
        .map(|t| match *t {
            Turn::Layers { face, depth, turns } => Turn::Layers { face, depth, turns: if turns == 2 { 2 } else { -turns } },
            w => w,
        })
        .collect()
}

/// The cube after `moves` made by hand (depth from the face, any depth up to n-1).
fn by_hand(n: usize, start: &[u8], moves: &[Turn]) -> Vec<u8> {
    let g = Geometry::new(n);
    let mut f = start.to_vec();
    for t in moves {
        let Turn::Layers { face, depth, turns } = *t else { continue };
        let power = match turns {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        let (face, depth) = (face as usize, depth as usize);
        if depth <= n / 2 {
            g.apply(&mut f, (face + 6 * (depth - 1)) * 3 + power);
        } else {
            // depth layers from a face = the whole cube, then n-depth layers from the opposite face back.
            let axis = [1, 0, 2, 1, 0, 2][face];
            let sign: i32 = if face < 3 { 1 } else { -1 };
            let q = (sign * [1, 2, 3][power] as i32).rem_euclid(4) as usize;
            f = g.turned(&f, &vec![axis; q]);
            let opposite = (face + 3) % 6;
            g.apply(&mut f, (opposite + 6 * (n - depth - 1)) * 3 + power);
        }
    }
    f
}

fn same_up_to_rotation(g: &Geometry, a: &[u8], b: &[u8]) -> bool {
    fastest::orientations().iter().any(|t| g.turned(a, t) == b)
}

fn starts(n: usize) -> Vec<RobotState> {
    vec![
        RobotState { lift: Lift::Level(0), flip_up: true },
        RobotState { lift: Lift::Level(0), flip_up: false },
        RobotState { lift: Lift::Scan, flip_up: false },
        RobotState { lift: Lift::Level(n as u8), flip_up: true },
        RobotState { lift: Lift::Level(1), flip_up: false },
    ]
}

fn run(cube: &mut PhysicalCube, ops: &[anycube::robot::multicuber::planner::Op]) {
    for op in ops {
        cube.apply_line(&op.line());
    }
}

#[test]
fn every_plan_does_what_the_moves_do() {
    let mut rng = Rng(0x5eed);
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        for trial in 0..30 {
            let moves = random_turns(n, 1 + rng.next(25), &mut rng);
            let s = starts(n)[trial % 5];
            for mode in [PlanMode::Legacy, PlanMode::Optimized] {
                let mut p = Planner::new(n, BRule::Legacy).with_timing(&timing).from_state(s);
                // As the driver does: from the scanning height the lift comes down first.
                p.lift_to(Lift::Level(0));
                match mode {
                    PlanMode::Legacy => p.solution(&moves),
                    PlanMode::Optimized => optimize::solution(&mut p, &moves, &timing),
                }
                let mut cube = PhysicalCube::new(n);
                cube.lift = s.lift;
                cube.flip_up = s.flip_up;
                run(&mut cube, &p.ops);
                let want = by_hand(n, &cube.g.solved(), &moves);
                assert!(same_up_to_rotation(&cube.g, &cube.facelets, &want), "{:?} {}x{}x{}: {:?}", mode, n, n, n, moves);
                assert!(cube.violations.is_empty(), "{:?}: {:?}", mode, cube.violations);
                // Parked as the firmware wants it at the end: lift down, flipper up.
                assert_eq!((cube.lift, cube.flip_up), (Lift::Level(0), true));
                // The rotator never turns with the lift down or at the scanning height.
                let mut lift = s.lift;
                for op in &p.ops {
                    match op.cmd {
                        Cmd::Lift(l) => lift = l,
                        Cmd::Rot(_) => assert!(!matches!(lift, Lift::Scan | Lift::Level(0))),
                        _ => {}
                    }
                }
                // Every move is turned by exactly one command, in order.
                let turned: Vec<usize> = p.ops.iter().filter_map(|o| o.mv.map(|m| m as usize)).collect();
                assert_eq!(turned, (0..moves.len()).collect::<Vec<_>>());
            }
        }
    }
}

#[test]
fn plans_solve_scrambled_cubes_and_optimized_is_never_slower() {
    let mut rng = Rng(0xc0ffee);
    let mut saved = 0.0;
    let mut total = 0.0;
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        for trial in 0..20 {
            let scramble = random_turns(n, 25, &mut rng);
            let solution = inverse(&scramble);
            let s = starts(n)[trial % 5];
            let mut legacy = Planner::new(n, BRule::Legacy).with_timing(&timing).from_state(s);
            legacy.lift_to(Lift::Level(0));
            legacy.solution(&solution);
            let mut opt = Planner::new(n, BRule::Legacy).with_timing(&timing).from_state(s);
            opt.lift_to(Lift::Level(0));
            optimize::solution(&mut opt, &solution, &timing);
            for p in [&legacy, &opt] {
                let mut cube = PhysicalCube::new(n);
                cube.facelets = by_hand(n, &cube.g.solved(), &scramble);
                cube.lift = s.lift;
                cube.flip_up = s.flip_up;
                run(&mut cube, &p.ops);
                assert!(cube.is_solved(), "{}x{}x{}: {:?}", n, n, n, scramble);
                assert!(cube.violations.is_empty(), "{:?}", cube.violations);
            }
            assert!(opt.ms <= legacy.ms + 1e-6, "optimized {:.0} ms > legacy {:.0} ms", opt.ms, legacy.ms);
            saved += legacy.ms - opt.ms;
            total += legacy.ms;
        }
    }
    eprintln!("optimizing planner: {:.1} % less robot time than legacy on the same moves", 100.0 * saved / total);
}

#[test]
fn orientations_are_the_24_rotations_and_moves_follow_them() {
    let g = Geometry::new(4);
    let os = fastest::orientations();
    assert_eq!(os.len(), 24);
    let ids: Vec<u8> = (0..96).collect();
    let distinct: std::collections::HashSet<Vec<u8>> = os.iter().map(|t| g.turned(&ids, t)).collect();
    assert_eq!(distinct.len(), 24);
    // A solution of the cube held otherwise, its moves mapped back, solves the cube as it is.
    let mut rng = Rng(9);
    for t in &os {
        let map = fastest::move_map(&g, t);
        let scramble: Vec<usize> = (0..20).map(|_| rng.next(g.move_count())).collect();
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let mut held = g.turned(&f, t);
        // The inverse scramble, written in the held frame: find for each move the one that maps to it.
        let inv: Vec<usize> = scramble.iter().rev().map(|&m| m / 3 * 3 + 2 - m % 3).collect();
        let held_moves: Vec<usize> = inv.iter().map(|&m| map.iter().position(|&x| x == m).unwrap()).collect();
        g.apply_seq(&mut held, &held_moves);
        assert!(g.is_solved(&held));
        let back: Vec<usize> = held_moves.iter().map(|&m| map[m]).collect();
        assert_eq!(back, inv);
    }
}

#[test]
fn costing_adds_up_what_the_commands_take() {
    let mut rng = Rng(77);
    let timing = TimeModel::for_size(3);
    for _ in 0..50 {
        let moves = random_turns(3, 20, &mut rng);
        let s = RobotState { lift: Lift::Level(0), flip_up: false };
        let mut full = Planner::new(3, BRule::Legacy).with_timing(&timing).from_state(s);
        full.solution(&moves);
        let mut cost = Planner::costing(3, BRule::Legacy, &timing).from_state(s);
        cost.solution(&moves);
        let sum: f64 = full.ops.iter().map(|o| timing.op_ms(o)).sum();
        assert!((cost.ms - sum).abs() < 1e-6 && (full.ms - sum).abs() < 1e-6);
        assert!(cost.ops.is_empty() && cost.count == full.ops.len());
    }
}

/// The camera sees the same robot position at every picture, and that position
/// at picture i holds what is at face i's position when the scan has ended.
#[test]
fn scan_pictures_show_the_faces_they_are_named_after() {
    use std::collections::BTreeSet;
    for n in [3usize, 4, 5] {
        let mut p = Planner::new(n, BRule::Legacy);
        p.scan(false);
        let nn = n * n;
        // Every sticker its own number, to follow it.
        let mut cube = PhysicalCube::new(n);
        cube.facelets = (0..6 * nn).map(|i| i as u8).collect();
        let mut seen: Vec<(char, Vec<u8>)> = Vec::new();
        for op in &p.ops {
            cube.apply_line(&op.line());
            if let Some(ph) = op.photo {
                seen.push((ph.face, cube.facelets.clone()));
            }
        }
        assert_eq!(seen.len(), 6);
        let face = |f: &[u8], k: usize| -> BTreeSet<u8> { f[k * nn..(k + 1) * nn].iter().copied().collect() };
        let mut camera = None;
        for (name, at) in &seen {
            let want = face(&cube.facelets, "URFDLB".find(*name).unwrap());
            let pos = (0..6).find(|&k| face(at, k) == want).unwrap_or_else(|| panic!("{}x{}x{}: picture of {} shows no whole face", n, n, n, name));
            assert_eq!(*camera.get_or_insert(pos), pos, "{}x{}x{}: the camera moved", n, n, n);
        }
        assert!(cube.violations.is_empty());
    }
}

/// The time model decides: of the orders of a pair of opposite turns the
/// cheaper is chosen, and over random cubes the chosen solution is never
/// slower than the solver's own shortest one.
#[test]
fn the_cheaper_plan_is_chosen() {
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
    let timing = TimeModel::for_size(3);
    let after_scan = RobotState { lift: Lift::Level(0), flip_up: false };
    let cost = |m: &[Turn], mode: PlanMode| {
        let mut p = Planner::new(3, BRule::Legacy).with_timing(&timing).from_state(after_scan);
        match mode {
            PlanMode::Legacy => p.solution(m),
            PlanMode::Optimized => optimize::solution(&mut p, m, &timing),
        }
        p.ms
    };
    // Orders: "U D" and "D U" are both offered.
    let o = fastest::orders(&[0, 9, 3, 12], 16);
    assert_eq!(o.len(), 4);
    let g = Geometry::new(3);
    let mut rng = Rng(4242);
    // Deterministic: no deadline, a cap far away.
    let opts = Options { search_ms: 1e9, max_solutions: 1_000_000, depth: 12, ..Options::default() };
    let mut wins = 0;
    for k in 0..12 {
        let scramble: Vec<String> = random_turns(3, 25, &mut rng).iter().map(Turn::name).collect();
        let c = check(3, Input::Moves(&scramble.join(" ")), 3).unwrap();
        let choice = fastest::choose(&c, &cost, &opts).unwrap();
        let mut f = c.facelets.clone();
        let idx: Vec<usize> = choice
            .moves
            .iter()
            .map(|t| match *t {
                Turn::Layers { face, turns, .. } => face as usize * 3 + match turns { 1 => 0, 2 => 1, _ => 2 },
                _ => unreachable!(),
            })
            .collect();
        g.apply_seq(&mut f, &idx);
        assert!(g.is_solved(&f));
        // The solver's own answer with the same first-phase depth (fast mode) is one of the candidates.
        let plain = anycube::api::solve(&c, anycube::api::Mode::Fast).unwrap();
        let plain_turns: Vec<Turn> = plain.moves.iter().map(|&m| Turn::from_index(m)).collect();
        let plain_ms = cost(&plain_turns, PlanMode::Legacy);
        assert!(choice.est_ms <= plain_ms + 1e-6, "cube {}: chosen {:.0} ms, plain {:.0} ms", k, choice.est_ms, plain_ms);
        assert!((cost(&choice.moves, PlanMode::Legacy) - choice.est_ms).abs() < 1e-6);
        assert!(choice.candidates >= choice.solutions && choice.solutions > 10, "{} solutions", choice.solutions);
        if choice.est_ms < plain_ms - 500.0 {
            wins += 1;
        }
    }
    assert!(wins >= 8, "the robot-time choice saved half a second on only {} of 12 cubes", wins);
}
