//! The overlap study (`robot::multicuber::overlap`, robots-spec/OVERLAP.md):
//! plans in which the flipper's empty moves are made while the lift or the
//! rotator moves. Held here: such a plan does to the cube what its moves
//! do, is command for command one the robot could make today, keeps to the
//! safety envelope at every millisecond of its timeline (checked from the
//! motions, not from the plan), takes exactly the time the planner counted,
//! and is never slower than the plan of today.
#![cfg(feature = "robots")]

use anycube::robot::multicuber::optimize::Space;
use anycube::robot::multicuber::overlap::{check, joinable, pair, timeline, MARGIN_MS};
use anycube::robot::multicuber::planner::{Cmd, Lift, Op, Planner, Remap, RobotState};
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

fn random_turns(n: usize, count: usize, rng: &mut Rng) -> Vec<Turn> {
    (0..count).map(|_| Turn::Layers { face: rng.next(6) as u8, depth: 1 + rng.next(n - 1) as u8, turns: [1, -1, 2][rng.next(3)] }).collect()
}

fn all_states(n: usize) -> Vec<RobotState> {
    let lifts = (0..=n as u8).map(Lift::Level).chain([Lift::Scan]);
    lifts.flat_map(|lift| [true, false].map(|flip_up| RobotState { lift, flip_up })).collect()
}

fn op(cmd: Cmd, lift_before: Lift) -> Op {
    Op { cmd, remap: Remap::None, photo: None, mv: None, lift_before, joined: false }
}

#[test]
fn overlapped_plans_are_right_safe_and_as_quick_as_counted() {
    let mut rng = Rng(0x0e1a9);
    for n in [3usize, 4, 5] {
        let timing = TimeModel::for_size(n);
        let (today, over) = (Space::new(n, &timing), Space::with_overlap(n, &timing));
        let (mut serial_ms, mut overlap_ms, mut pairs) = (0.0, 0.0, 0);
        for (k, &start) in all_states(n).iter().cycle().take(300).enumerate() {
            let moves = random_turns(n, 1 + k % 25, &mut rng);
            let mut p = Planner::new(n).with_timing(&timing).from_state(start);
            over.solution(&mut p, &moves);
            // The same change to the cube as today's plan of the same moves.
            let mut q = Planner::new(n).with_timing(&timing).from_state(start);
            today.solution(&mut q, &moves);
            let run = |ops: &[Op]| {
                let mut cube = PhysicalCube::new(n);
                cube.scramble(k as u64 + 1, 30);
                (cube.lift, cube.flip_up) = (start.lift, start.flip_up);
                for op in ops {
                    cube.apply_line(&op.line());
                }
                assert!(cube.violations.is_empty(), "{:?}", cube.violations);
                assert_eq!(cube.lift, Lift::Level(0));
                cube
            };
            let (a, b) = (run(&p.ops), run(&q.ops));
            assert!(
                anycube::robot::fastest::orientations().iter().any(|t| a.g.turned(&a.facelets, t) == b.facelets),
                "{}x{}x{} from {:?}: the overlapped plan of {:?} leaves another cube",
                n, n, n, start, moves
            );
            // The envelope, from the motions.
            let (spans, total) = timeline(&timing, &p.ops);
            let broken = check(&timing, start, &p.ops, &spans);
            assert!(broken.is_empty(), "{}x{}x{} from {:?}: {:?}", n, n, n, start, broken);
            // The time the planner counted, the time `cost` says, the time of the timeline: one time.
            assert!((total - p.ms).abs() < 1e-6, "timeline {:.2} ms, planner {:.2} ms", total, p.ms);
            assert!((over.cost(start, &moves) - p.ms).abs() < 0.5, "cost {:.2} ms, planner {:.2} ms", over.cost(start, &moves), p.ms);
            assert!(p.ms <= q.ms + 1e-6, "overlapped {:.0} ms, today {:.0} ms", p.ms, q.ms);
            // Today's plan has no pair, and its timeline is the plain sum.
            assert!(q.ops.iter().all(|o| !o.joined));
            assert!((timeline(&timing, &q.ops).1 - q.ms).abs() < 1e-6);
            assert!(check(&timing, start, &q.ops, &timeline(&timing, &q.ops).0).is_empty());
            pairs += p.ops.iter().filter(|o| o.joined).count();
            serial_ms += q.ms;
            overlap_ms += p.ms;
        }
        assert!(pairs > 100 && overlap_ms < serial_ms, "{} pairs", pairs);
        eprintln!("{0}x{0}x{0}: {1} pairs in 300 plans, {2:.1} % less robot time than the plans of today", n, pairs, 100.0 * (1.0 - overlap_ms / serial_ms));
    }
}

/// The checker does see what it is there for: pairs outside the envelope are reported.
#[test]
fn the_check_reports_what_is_outside_the_envelope() {
    let timing = TimeModel::for_size(3);
    let start = RobotState { lift: Lift::Level(0), flip_up: true };
    let joined = |mut o: Op| {
        o.joined = true;
        o
    };
    // Going up, the flipper started with the lift instead of after the cube is clear.
    let ops = [op(Cmd::Lift(Lift::Level(1)), Lift::Level(0)), joined(op(Cmd::Flip { up: false, normal: false }, Lift::Level(1)))];
    let (mut spans, _) = timeline(&timing, &ops);
    assert!(check(&timing, start, &ops, &spans).is_empty());
    assert!(spans[1].start >= timing.lift_time_to(Lift::Level(0), Lift::Level(1), 200.0) + MARGIN_MS - 1e-9);
    let len = spans[1].end - spans[1].start;
    spans[1].start = 0.0;
    spans[1].end = len;
    assert!(!check(&timing, start, &ops, &spans).is_empty());
    // Going down, the lift started with the flipper instead of waiting for it.
    let raised = RobotState { lift: Lift::Level(1), flip_up: true };
    let ops = [op(Cmd::Flip { up: false, normal: false }, Lift::Level(1)), joined(op(Cmd::Lift(Lift::Level(0)), Lift::Level(1)))];
    let (mut spans, total) = timeline(&timing, &ops);
    assert!(check(&timing, raised, &ops, &spans).is_empty());
    let serial = timing.op_ms(&ops[0]) + timing.op_ms(&ops[1]);
    assert!(total < serial, "the pair {:.0} ms, one after the other {:.0} ms", total, serial);
    // (The flipper 60 ms late: still moving when the cube comes down past the scanning height.)
    spans[0].start += 60.0;
    spans[0].end += 60.0;
    assert!(!check(&timing, raised, &ops, &spans).is_empty());
    // The lift and the rotator together; a carrying flip with anything.
    let ops = [op(Cmd::Lift(Lift::Level(2)), Lift::Level(1)), op(Cmd::Rot(1), Lift::Level(2))];
    let (mut spans, _) = timeline(&timing, &ops);
    spans[1].start -= 50.0;
    assert!(!check(&timing, raised, &ops, &spans).is_empty());
    let ops = [op(Cmd::Flip { up: false, normal: false }, Lift::Level(0)), op(Cmd::Lift(Lift::Level(1)), Lift::Level(0))];
    let (mut spans, _) = timeline(&timing, &ops);
    spans[1].start -= 50.0;
    assert!(!check(&timing, start, &ops, &spans).is_empty());
    assert!(pair(&timing, &ops[0], &ops[1]).is_none(), "a carrying flip is never part of a pair");
}

/// Which pairs may be started together: the rules of the envelope, case by case.
#[test]
fn only_the_pairs_of_the_envelope_are_joined() {
    for n in [3u8, 4, 5] {
        let top = Lift::Level(n);
        for flip_up in [true, false] {
            // Nothing with the lift down or at the scanning height but going up from 0.
            assert_eq!(joinable(n, Lift::Level(0), flip_up, Cmd::Rot(1)), None);
            assert_eq!(joinable(n, Lift::Scan, flip_up, Cmd::Lift(Lift::Level(0))), None);
            assert_eq!(joinable(n, Lift::Level(0), flip_up, Cmd::Lift(Lift::Scan)), None);
            assert_eq!(joinable(n, Lift::Level(0), flip_up, Cmd::Lift(Lift::Level(1))), Some(false));
            // A turn at a turning level, a move between levels, the way down from one.
            assert_eq!(joinable(n, Lift::Level(1), flip_up, Cmd::Rot(2)), Some(false));
            assert_eq!(joinable(n, Lift::Level(1), flip_up, Cmd::Lift(Lift::Level(2))), Some(true));
            assert_eq!(joinable(n, Lift::Level(1), flip_up, Cmd::Lift(Lift::Level(0))), Some(true));
            // Where the flipper's side matters today, nothing is joined.
            assert_eq!(joinable(n, top, flip_up, Cmd::Rot(2)), None);
            assert_eq!(joinable(n, top, flip_up, Cmd::Lift(Lift::Level(0))), None);
        }
        // At the top level the flipper only ever goes DOWN.
        assert_eq!(joinable(n, top, true, Cmd::Rot(1)), Some(false));
        assert_eq!(joinable(n, top, false, Cmd::Rot(1)), None);
        assert_eq!(joinable(n, Lift::Level(0), false, Cmd::Lift(top)), None);
    }
}
