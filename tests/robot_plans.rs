//! MultiCuber's plans on the sticker model. The planner that drives the
//! robot (`optimize`) is held to this, for sizes 3 to 5, from every state
//! the robot can be in (after a scan, after a solve, after a stop anywhere):
//!
//! - every command is sent in a state in which the 2018 planner sends it
//!   (`planner::allowed`, which is shown to be exactly what the 2018
//!   planner produces, no more and no less);
//! - the plan does what the moves do, and solves a scrambled cube;
//! - it ends with the lift down, turns each move with one rotator turn in
//!   order, takes what the cost function says, and never more than the
//!   2018 plan of the same moves;
//! - a scan shows each face once, with the lift down and the flipper DOWN,
//!   names each picture after the place its face has at the end, with the
//!   turn that the 2018 scan has for it, and leaves the cube as it was.
//!
//! And the time model really decides which solution is taken.
#![cfg(feature = "robots")]

use anycube::api::{check, Input};
use anycube::cuben::Geometry;
use anycube::robot::fastest::{self, Options};
use anycube::robot::multicuber::legacy::{BRule, Legacy, SCAN_PHOTOS};
use anycube::robot::multicuber::optimize::{name_photos, Space};
use anycube::robot::multicuber::planner::{allowed, Cmd, Lift, Op, Planner, RobotState, CAMERA};
use anycube::robot::multicuber::timing::TimeModel;
use anycube::robot::physical::PhysicalCube;
use anycube::robot::Turn;
use std::collections::BTreeSet;

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Face turns, on larger cubes also of several layers; with `whole`, now and then the solver's x, y, z.
fn random_turns(n: usize, count: usize, whole: bool, rng: &mut Rng) -> Vec<Turn> {
    (0..count)
        .map(|_| {
            if whole && rng.next(12) == 0 {
                return Turn::Whole { axis: rng.next(3) as u8, turns: [1, -1, 2][rng.next(3)] };
            }
            let depth = if n > 3 && rng.next(10) < 4 { 1 + rng.next(n - 1) } else { 1 };
            Turn::Layers { face: rng.next(6) as u8, depth: depth as u8, turns: [1, -1, 2][rng.next(3)] }
        })
        .collect()
}

fn inverse(moves: &[Turn]) -> Vec<Turn> {
    let back = |t: i8| if t == 2 { 2 } else { -t };
    moves
        .iter()
        .rev()
        .map(|t| match *t {
            Turn::Layers { face, depth, turns } => Turn::Layers { face, depth, turns: back(turns) },
            Turn::Whole { axis, turns } => Turn::Whole { axis, turns: back(turns) },
        })
        .collect()
}

/// The cube after `moves` made by hand (depth from the face, any depth up to n-1; x, y, z turn it whole).
fn by_hand(n: usize, start: &[u8], moves: &[Turn]) -> Vec<u8> {
    let g = Geometry::new(n);
    let mut f = start.to_vec();
    for t in moves {
        let (face, depth, turns) = match *t {
            Turn::Whole { axis, turns } => {
                f = g.turned(&f, &vec![axis as usize; (turns as i32).rem_euclid(4) as usize]);
                continue;
            }
            Turn::Layers { face, depth, turns } => (face as usize, depth as usize, turns),
        };
        let power = match turns {
            1 => 0,
            2 => 1,
            _ => 2,
        };
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

/// Every state the robot can be in: each lift position with the flipper on either side.
fn all_states(n: usize) -> Vec<RobotState> {
    let lifts = (0..=n as u8).map(Lift::Level).chain([Lift::Scan]);
    lifts.flat_map(|lift| [true, false].map(|flip_up| RobotState { lift, flip_up })).collect()
}

const AFTER_SCAN: RobotState = RobotState { lift: Lift::Level(0), flip_up: false };
const PARKED: RobotState = RobotState { lift: Lift::Level(0), flip_up: true };

fn cube_at(n: usize, s: RobotState) -> PhysicalCube {
    let mut cube = PhysicalCube::new(n);
    cube.lift = s.lift;
    cube.flip_up = s.flip_up;
    cube
}

fn run(cube: &mut PhysicalCube, ops: &[Op]) {
    for op in ops {
        cube.apply_line(&op.line());
    }
}

fn time(timing: &TimeModel, ops: &[Op]) -> f64 {
    ops.iter().map(|o| timing.op_ms(o)).sum()
}

/// The commands the 2018 planner sends, each with the state it is sent in.
fn contexts(start: RobotState, ops: &[Op], into: &mut BTreeSet<(Lift2, bool, String)>) {
    let (mut lift, mut flip_up) = (start.lift, start.flip_up);
    for op in ops {
        if !matches!(op.cmd, Cmd::State) {
            into.insert((key(lift), flip_up, op.line().replace(" NORMAL", "")));
        }
        match op.cmd {
            Cmd::Lift(l) => lift = l,
            Cmd::Flip { up, .. } => flip_up = up,
            _ => {}
        }
    }
}

type Lift2 = u8;

fn key(l: Lift) -> Lift2 {
    match l {
        Lift::Level(k) => k,
        Lift::Scan => 255,
    }
}

/// `allowed` lets through exactly the commands, in exactly the states, that the 2018
/// planner (robot.py's, as it runs: scans and solves one after the other) sends.
#[test]
fn allowed_is_exactly_what_the_2018_planner_sends() {
    for n in [3usize, 4, 5] {
        let mut seen = BTreeSet::new();
        let mut rng = Rng(0x2018 + n as u64);
        for trial in 0..3000 {
            // As robot.py runs it: from the parked robot or after a scan, scans and solves in turn.
            let start = if trial % 2 == 0 { PARKED } else { AFTER_SCAN };
            let mut l = Legacy::new(Planner::new(n).from_state(start), BRule::Legacy);
            for step in 0..3 {
                if (trial / 2 + step) % 2 == 0 {
                    l.scan(trial % 5 == 0);
                } else {
                    let moves: Vec<Turn> = (0..30)
                        .map(|_| Turn::Layers { face: rng.next(6) as u8, depth: 1 + rng.next(n - 1) as u8, turns: [1, -1, 2][rng.next(3)] })
                        .collect();
                    l.solution(&moves);
                }
            }
            l.park();
            contexts(start, &l.p.ops, &mut seen);
        }
        let lifts: Vec<Lift> = (0..=n as u8).map(Lift::Level).chain([Lift::Scan]).collect();
        let mut cmds: Vec<Cmd> = lifts.iter().map(|&l| Cmd::Lift(l)).collect();
        cmds.extend([Cmd::Flip { up: true, normal: false }, Cmd::Flip { up: false, normal: false }, Cmd::Rot(1), Cmd::Rot(-1), Cmd::Rot(2)]);
        let mut count = 0;
        for &lift in &lifts {
            for flip_up in [true, false] {
                for &cmd in &cmds {
                    let line = Op { cmd, remap: anycube::robot::multicuber::planner::Remap::None, photo: None, mv: None, lift_before: lift }.line();
                    let sent = seen.contains(&(key(lift), flip_up, line.clone()));
                    assert_eq!(
                        allowed(n as u8, lift, flip_up, cmd),
                        sent,
                        "{}x{}x{}: {} with the lift at {} and the flipper {}: the 2018 planner {} it",
                        n, n, n, line, lift.arg(), if flip_up { "UP" } else { "DOWN" }, if sent { "sends" } else { "never sends" }
                    );
                    count += sent as usize;
                }
            }
        }
        assert_eq!(count, seen.len());
        // Nothing above the top level, no other turn.
        assert!(!allowed(n as u8, Lift::Level(0), true, Cmd::Lift(Lift::Level(n as u8 + 1))));
        assert!(!allowed(n as u8, Lift::Level(1), true, Cmd::Rot(3)));
    }
}

/// From every state, random moves: safe, right, ending with the lift down, as quick as the cost says.
#[test]
fn every_plan_is_safe_and_does_what_the_moves_do() {
    let mut rng = Rng(0x5eed);
    let mut plans = 0;
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        let space = Space::new(n, &timing);
        for (k, &s) in all_states(n).iter().cycle().take(40 * all_states(n).len()).enumerate() {
            let moves = random_turns(n, rng.next(31), k % 3 == 0, &mut rng);
            let mut p = Planner::new(n).with_timing(&timing).from_state(s);
            space.solution(&mut p, &moves);
            let mut cube = cube_at(n, s);
            run(&mut cube, &p.ops);
            let want = by_hand(n, &cube.g.solved(), &moves);
            assert!(same_up_to_rotation(&cube.g, &cube.facelets, &want), "{}x{}x{} from {:?}: {:?}", n, n, n, s, moves);
            assert!(cube.violations.is_empty(), "from {:?}: {:?}", s, cube.violations);
            // The cube rests in the flipper at the end, the lift down.
            assert_eq!(cube.lift, Lift::Level(0));
            // No back and forth of the flipper at the end.
            assert!(!matches!(p.ops.last().map(|o| o.cmd), Some(Cmd::Flip { .. })) || moves.is_empty(), "{:?}", p.ops.last());
            // Every move is turned by exactly one rotator turn, in order.
            let layer_moves: Vec<usize> = (0..moves.len()).filter(|&i| matches!(moves[i], Turn::Layers { .. })).collect();
            let turned: Vec<usize> = p.ops.iter().filter_map(|o| o.mv.map(|m| m as usize)).collect();
            assert_eq!(turned, layer_moves);
            assert!(p.ops.iter().all(|o| o.mv.is_none() || matches!(o.cmd, Cmd::Rot(_))));
            // The plan takes what the cost function said, to the rounding of the search (0.01 ms a command).
            let ms = time(&timing, &p.ops);
            assert!((ms - space.cost(s, &moves)).abs() < 0.01 * (p.ops.len() + 1) as f64, "plan {} ms, cost {} ms", ms, space.cost(s, &moves));
            assert!((ms - p.ms).abs() < 1e-6);
            plans += 1;
        }
    }
    assert!(plans >= 1000);
}

/// Scrambled cubes come out solved, from where a scan and a solve leave the robot and from
/// everywhere else, and never slower than the 2018 plan of the same moves.
#[test]
fn plans_solve_scrambled_cubes_and_are_never_slower_than_2018() {
    let mut rng = Rng(0xc0ffee);
    let (mut saved, mut total) = (0.0, 0.0);
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        let space = Space::new(n, &timing);
        let mut states = vec![AFTER_SCAN, PARKED];
        states.extend(all_states(n));
        for (k, &s) in states.iter().cycle().take(20 * states.len()).enumerate() {
            let scramble = random_turns(n, 25, k % 4 == 0, &mut rng);
            let solution = inverse(&scramble);
            let mut p = Planner::new(n).with_timing(&timing).from_state(s);
            space.solution(&mut p, &solution);
            let mut cube = cube_at(n, s);
            cube.facelets = by_hand(n, &cube.g.solved(), &scramble);
            run(&mut cube, &p.ops);
            assert!(cube.is_solved(), "{}x{}x{} from {:?}: {:?}", n, n, n, s, scramble);
            assert!(cube.violations.is_empty(), "{:?}", cube.violations);
            // The 2018 planner starts from the lift down (robot.py never starts elsewhere).
            if s.lift == Lift::Level(0) {
                let mut l = Legacy::new(Planner::new(n).with_timing(&timing).from_state(s), BRule::Legacy);
                l.solution(&solution);
                assert!(p.ms <= l.p.ms + 1e-6, "{:.0} ms > the 2018 plan's {:.0} ms", p.ms, l.p.ms);
                saved += l.p.ms - p.ms;
                total += l.p.ms;
            }
        }
    }
    eprintln!("the planner: {:.1} % less robot time than the 2018 plans of the same moves", 100.0 * saved / total);
}

#[test]
fn parking_is_safe_from_everywhere() {
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        let space = Space::new(n, &timing);
        for s in all_states(n) {
            let mut p = Planner::new(n).with_timing(&timing).from_state(s);
            space.park(&mut p);
            let mut cube = cube_at(n, s);
            run(&mut cube, &p.ops);
            assert!(cube.violations.is_empty(), "from {:?}: {:?}", s, cube.violations);
            assert_eq!((cube.lift, cube.flip_up), (Lift::Level(0), true));
            assert!(p.ops.len() <= 4 && (s != PARKED || p.ops.is_empty()));
        }
    }
}

/// The pictures of a scan as the sticker model sees them: (name, what the camera's place holds).
fn pictures(n: usize, start: RobotState, ops: &[Op], cube: &mut PhysicalCube) -> Vec<(char, u16, Vec<u8>)> {
    let nn = n * n;
    let camera = CAMERA as usize;
    let mut seen = Vec::new();
    let (mut lift, mut flip_up) = (start.lift, start.flip_up);
    for op in ops {
        cube.apply_line(&op.line());
        match op.cmd {
            Cmd::Lift(l) => lift = l,
            Cmd::Flip { up, .. } => flip_up = up,
            _ => {}
        }
        if let Some(ph) = op.photo {
            // The camera's pose: the cube in the flipper, the flipper DOWN.
            assert_eq!((lift, flip_up), (Lift::Level(0), false), "a picture with the lift at {:?}", lift);
            assert_eq!(ph.index as usize, seen.len());
            seen.push((ph.face, ph.rotate, cube.facelets[camera * nn..(camera + 1) * nn].to_vec()));
        }
    }
    seen
}

/// The 2018 scan, its pictures named anew by following the stickers: the 2018 names, and the
/// 2018 turns but for U and D, which come out half a turn from robot.py's. That half turn is
/// the one MultiCuber's real photos need to make a cube (FAST_SCAN.md: `{"U":2,"D":2}`), so
/// the naming the planner's own scans use is the right way up for anycube's reading.
#[test]
fn the_2018_scan_is_named_as_in_2018_with_u_and_d_the_right_way_up() {
    for n in [3usize, 4, 5] {
        for start in [PARKED, AFTER_SCAN] {
            let mut l = Legacy::new(Planner::new(n).from_state(start), BRule::Legacy);
            l.scan(false);
            let mut ops = l.p.ops;
            for op in &mut ops {
                if let Some(ph) = op.photo.as_mut() {
                    (ph.face, ph.rotate) = ('?', 0);
                }
            }
            name_photos(n, start, &mut ops);
            let named: Vec<(char, u16)> = ops.iter().filter_map(|o| o.photo.map(|p| (p.face, p.rotate))).collect();
            let want: Vec<(char, u16)> = SCAN_PHOTOS.iter().map(|&(face, rotate)| (face, if "UD".contains(face) { (rotate + 180) % 360 } else { rotate })).collect();
            assert_eq!(named, want, "{}x{}x{}", n, n, n);
            assert_eq!(want, [('U', 90), ('R', 180), ('D', 270), ('L', 0), ('B', 0), ('F', 0)]);
        }
    }
}

#[test]
fn a_scan_shows_every_face_once_and_names_it_right() {
    let mut report = Vec::new();
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        let space = Space::new(n, &timing);
        let nn = n * n;
        for seated in [true, false] {
            for s in all_states(n) {
                let mut p = Planner::new(n).with_timing(&timing).from_state(s);
                space.scan(&mut p, seated);
                let mut ops = p.ops;
                name_photos(n, s, &mut ops);
                // Every sticker its own number, to follow it.
                let mut cube = cube_at(n, s);
                cube.facelets = (0..6 * nn).map(|i| i as u8).collect();
                let before = cube.facelets.clone();
                let seen = pictures(n, s, &ops, &mut cube);
                assert!(cube.violations.is_empty(), "from {:?}: {:?}", s, cube.violations);
                assert_eq!(seen.len(), 6);
                assert_eq!(seen.iter().map(|p| p.0).collect::<BTreeSet<char>>().len(), 6, "six different faces");
                // The scan ends at its last picture, where the solve starts: lift down, flipper DOWN.
                assert_eq!((cube.lift, cube.flip_up), (AFTER_SCAN.lift, AFTER_SCAN.flip_up));
                assert!(ops.last().unwrap().photo.is_some());
                // The cube is turned, not scrambled.
                assert!(same_up_to_rotation(&cube.g, &before, &cube.facelets));
                for (name, rotate, shown) in &seen {
                    // The picture, turned as told, is the face of that name as the scan leaves it.
                    let face = "URFDLB".find(*name).unwrap();
                    let mut upright = shown.clone();
                    for _ in 0..rotate / 90 {
                        // A quarter turn clockwise of the picture: row r, column c comes from row n-1-c, column r.
                        upright = (0..nn).map(|i| upright[(n - 1 - i % n) * n + i / n]).collect();
                    }
                    assert_eq!(upright, &cube.facelets[face * nn..(face + 1) * nn], "{}x{}x{} from {:?}: the picture of {}", n, n, n, s, name);
                }
                if seated {
                    // Each picture right after the flipper tipped the cube in front of the camera.
                    for op in ops.iter().filter(|o| o.photo.is_some()) {
                        assert!(op.carries() && matches!(op.cmd, Cmd::Flip { up: false, .. }), "{:?}", op);
                    }
                }
                if s == PARKED {
                    let mut l = Legacy::new(Planner::new(n).with_timing(&timing).from_state(s), BRule::Legacy);
                    l.scan(false);
                    let ms = time(&timing, &ops);
                    assert!(ms <= l.p.ms + 1e-6);
                    report.push(format!(
                        "{0}x{0}x{0} scan from the parked robot, {1}: {2:.0} ms in {3} commands (2018: {4:.0} ms in {5}): {6}",
                        n,
                        if seated { "pictures after a tip" } else { "pictures anywhere" },
                        ms,
                        ops.len(),
                        l.p.ms,
                        l.p.ops.len(),
                        ops.iter().map(|o| o.line() + &o.photo.map_or(String::new(), |p| format!(" [{} {}]", p.face, p.rotate))).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
    }
    for line in report {
        eprintln!("{}", line);
    }
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

/// The time model decides: of the orders of a pair of opposite turns the
/// cheaper is chosen, and over random cubes the chosen solution is never
/// slower than the solver's own shortest one.
#[test]
fn the_cheaper_plan_is_chosen() {
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
    let timing = TimeModel::for_size(3);
    let space = Space::new(3, &timing);
    let cost = |m: &[Turn]| space.cost(AFTER_SCAN, m);
    // Orders: "U D" and "D U" are both offered.
    let o = fastest::orders(&[0, 9, 3, 12], 16);
    assert_eq!(o.len(), 4);
    let g = Geometry::new(3);
    let mut rng = Rng(4242);
    // Deterministic: no deadline, a cap far away.
    let opts = Options { search_ms: 1e9, total_ms: 1e9, max_solutions: 1_000_000, depth: 12, ..Options::default() };
    let (mut wins, mut saved) = (0, 0.0);
    for k in 0..12 {
        let scramble: Vec<String> = random_turns(3, 25, false, &mut rng).iter().map(Turn::name).collect();
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
        let plain_ms = cost(&plain_turns);
        assert!(choice.est_ms <= plain_ms + 1e-6, "cube {}: chosen {:.0} ms, plain {:.0} ms", k, choice.est_ms, plain_ms);
        assert!((cost(&choice.moves) - choice.est_ms).abs() < 1e-6);
        assert!(choice.candidates >= choice.solutions && choice.solutions > 10, "{} solutions", choice.solutions);
        saved += plain_ms - choice.est_ms;
        if choice.est_ms < plain_ms - 500.0 {
            wins += 1;
        }
    }
    eprintln!("the robot-time choice: {:.0} ms a cube less than the solver's own answer, half a second or more on {} of 12", saved / 12.0, wins);
    assert!(wins >= 3 && saved / 12.0 > 300.0, "the robot-time choice saved half a second on only {} of 12 cubes", wins);
}
