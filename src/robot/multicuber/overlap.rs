//! A study, not what drives the robot: motions that overlap.
//!
//! The firmware of today runs one command at a time, so a run takes the sum
//! of its commands. This module says what a run would take if the flipper,
//! when the cube is off it, could move while the lift or the rotator moves
//! (robots-spec/OVERLAP.md proposes the firmware protocol for it): a pair
//! of commands "started together" (`Op::joined`), the time of such a pair
//! (`pair`), which pairs are safe (`joinable`), when every command of a run
//! then starts and ends (`timeline`), and a check of a run's timeline
//! against the safety envelope that looks at the motions themselves and
//! not at how they were planned (`check`). `Space::with_overlap` plans
//! with these pairs.
//!
//! The safety envelope assumed here, for the owner to confirm on the robot:
//!
//! 1. Only the flipper moving alone ever overlaps anything. A flipper move
//!    that carries the cube (lift at 0) has the robot to itself.
//! 2. The flipper moves alone only while the lift is at or above the
//!    scanning height (`Cal::scan`, 200 steps), and in a pair with
//!    `MARGIN_MS` to spare at both ends. That is the lowest height at which the flipper is moved
//!    alone today, so the cube is clear of it there. Going up from 0, the
//!    flipper starts once the lift has passed that height; going down to 0,
//!    the lift starts late enough for the flipper to have come to rest
//!    before the lift passes it.
//! 3. The lift and the rotator never move at the same time (the rotator
//!    holds the layers the lift has brought into it).
//! 4. A command that today is only sent with the flipper on one side (a
//!    half turn of the whole cube at level n, the lift coming down from
//!    level n: flipper DOWN) waits for the flipper to be at rest there: it
//!    is never part of a pair.
//! 5. Each command of a pair is one that `planner::allowed` lets through in
//!    the state it is sent in, in the order the pair is listed: a run with
//!    overlaps is, command for command, a run the robot could make today.
//!
//! So three overlaps are scheduled: the flipper's empty move with a
//! rotator turn (at a turning level), with a lift move between two raised
//! positions, and with the part of a lift move from or to 0 that is above
//! the scanning height.

use super::planner::{allowed, Cmd, Lift, Op, RobotState};
use super::timing::TimeModel;

/// Kept between a flipper move and the lift passing the scanning height (ms).
pub const MARGIN_MS: f64 = 20.0;

/// The order in which a lift move or rotator turn and the flipper's move to
/// the other side are sent when they are started together, if they may be:
/// `Some(true)` the flipper's first. `lift` and `flip_up`: the state before both.
pub fn joinable(n: u8, lift: Lift, flip_up: bool, main: Cmd) -> Option<bool> {
    let flip = Cmd::Flip { up: !flip_up, normal: false };
    // Whatever side the flipper is on (rule 4).
    let either = |from: Lift, cmd: Cmd| allowed(n, from, true, cmd) && allowed(n, from, false, cmd);
    match main {
        Cmd::Rot(_) => (lift != Lift::Level(0) && lift != Lift::Scan && either(lift, main) && allowed(n, lift, flip_up, flip)).then_some(false),
        Cmd::Lift(to) => {
            if !either(lift, main) || lift == Lift::Scan || to == Lift::Scan {
                // (To and from the scanning height nothing could overlap: rule 2 leaves no time.)
                return None;
            }
            if lift == Lift::Level(0) {
                // Going up: the flipper moves once the cube is clear, as if sent after the lift.
                allowed(n, to, flip_up, flip).then_some(false)
            } else if allowed(n, lift, flip_up, flip) {
                Some(true)
            } else {
                (to != Lift::Level(0) && allowed(n, to, flip_up, flip)).then_some(false)
            }
        }
        _ => None,
    }
}

/// A pair started together: when each starts and when both are done (ms
/// from the pair's start; `end` with the two commands' overheads).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    pub main_start: f64,
    pub flip_start: f64,
    pub end: f64,
}

/// The times of two commands started together, in either order: a lift
/// move or a rotator turn, and a flipper move without the cube. `None` if
/// they are not such a pair.
pub fn pair(t: &TimeModel, a: &Op, b: &Op) -> Option<Pair> {
    let (main, flip) = match (a.cmd, b.cmd) {
        (Cmd::Flip { .. }, Cmd::Lift(_) | Cmd::Rot(_)) => (b, a),
        (Cmd::Lift(_) | Cmd::Rot(_), Cmd::Flip { .. }) => (a, b),
        _ => return None,
    };
    if flip.carries() {
        return None;
    }
    let main_ms = t.op_ms(main) - t.overhead_ms;
    let flip_ms = t.flip_move_ms(false);
    let clear = t.cal.scan as f64;
    let (main_start, flip_start) = match main.cmd {
        Cmd::Lift(to) if main.lift_before == Lift::Level(0) => {
            // Up from 0: the flipper waits for the cube to be clear.
            (0.0, t.lift_time_to(main.lift_before, to, clear) + MARGIN_MS)
        }
        Cmd::Lift(to) if to == Lift::Level(0) => {
            // Down to 0: the lift waits so that the flipper is at rest before the cube comes near.
            ((flip_ms + MARGIN_MS - t.lift_time_to(main.lift_before, to, clear)).max(0.0), 0.0)
        }
        _ => (0.0, 0.0),
    };
    Some(Pair { main_start, flip_start, end: (main_start + main_ms).max(flip_start + flip_ms) + 2.0 * t.overhead_ms })
}

/// One command's motion in a run: from `start` to `end` (ms from the run's start).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

/// When every command of a run moves, and the run's time: commands one
/// after the other, but for the pairs started together.
pub fn timeline(t: &TimeModel, ops: &[Op]) -> (Vec<Span>, f64) {
    let mut spans = Vec::with_capacity(ops.len());
    let mut clock = 0.0;
    let mut i = 0;
    while i < ops.len() {
        let op = &ops[i];
        let motion = |op: &Op| (t.op_ms(op) - t.overhead_ms).max(0.0);
        match ops.get(i + 1).filter(|next| next.joined).and_then(|next| pair(t, op, next).map(|p| (next, p))) {
            Some((next, p)) => {
                let starts = if matches!(op.cmd, Cmd::Flip { .. }) { (p.flip_start, p.main_start) } else { (p.main_start, p.flip_start) };
                spans.push(Span { start: clock + starts.0, end: clock + starts.0 + motion(op) });
                spans.push(Span { start: clock + starts.1, end: clock + starts.1 + motion(next) });
                clock += p.end;
                i += 2;
            }
            None => {
                spans.push(Span { start: clock, end: clock + motion(op) });
                clock += t.op_ms(op);
                i += 1;
            }
        }
    }
    (spans, clock)
}

/// What a run's timeline does against the safety envelope (the rules at
/// the top), from the motions alone: where the lift is at every
/// millisecond, when the flipper and the rotator move. Empty when all is
/// well. `start`: the robot's state before the run.
pub fn check(t: &TimeModel, start: RobotState, ops: &[Op], spans: &[Span]) -> Vec<String> {
    let n = t.cal.n as u8;
    let mut out = Vec::new();
    let clear = t.cal.scan as f64;
    // The lift's moves, to ask where it is at a time.
    let lifts: Vec<(Span, Lift, Lift)> = ops
        .iter()
        .zip(spans)
        .filter_map(|(op, s)| match op.cmd {
            Cmd::Lift(to) => Some((*s, op.lift_before, to)),
            _ => None,
        })
        .collect();
    let height = |at: f64| -> f64 {
        let mut now = t.cal.height(start.lift) as f64;
        for &(s, from, to) in &lifts {
            if at >= s.end {
                now = t.cal.height(to) as f64;
            } else if at > s.start {
                return t.lift_height_at(from, to, at - s.start);
            } else {
                break;
            }
        }
        now
    };
    let overlap = |a: &Span, b: &Span| a.start < b.end - 1e-9 && b.start < a.end - 1e-9;
    let name = |i: usize| format!("{} (command {})", ops[i].line(), i);
    let mut state = start;
    for (i, (op, s)) in ops.iter().zip(spans).enumerate() {
        if !allowed(n, state.lift, state.flip_up, op.cmd) {
            out.push(format!("{} is not sent with the lift at {} and the flipper {}", name(i), state.lift.arg(), if state.flip_up { "UP" } else { "DOWN" }));
        }
        let moving = |kind: fn(&Cmd) -> bool| (0..ops.len()).filter(move |&j| j != i && kind(&ops[j].cmd) && overlap(s, &spans[j]));
        let is_flip = |c: &Cmd| matches!(c, Cmd::Flip { .. });
        let is_main = |c: &Cmd| matches!(c, Cmd::Lift(_) | Cmd::Rot(_));
        match op.cmd {
            Cmd::Flip { .. } => {
                for j in moving(is_flip) {
                    out.push(format!("{} while the flipper still makes {}", name(i), name(j)));
                }
                if op.carries() {
                    // Rule 1: with the cube in it the flipper moves alone, the lift at rest at 0.
                    for j in moving(is_main) {
                        out.push(format!("{} carries the cube while {} moves", name(i), name(j)));
                    }
                    if height(s.start) != 0.0 || height(s.end) != 0.0 {
                        out.push(format!("{} carries the cube with the lift not down", name(i)));
                    }
                } else {
                    // Rule 2: the cube clear of the flipper all through; with the margin when the
                    // lift moves during it (one after the other, as today, there is nothing to keep apart).
                    let paired = moving(is_main).next().is_some();
                    let margin = if paired { MARGIN_MS - 0.5 } else { -0.5 };
                    let mut at = s.start - margin;
                    while at < s.end + margin {
                        if height(at) < clear - 0.5 {
                            out.push(format!("{} moves the flipper with the lift at {:.0} steps, {:.0} ms into the run", name(i), height(at), at));
                            break;
                        }
                        at += 1.0;
                    }
                }
            }
            Cmd::Lift(_) | Cmd::Rot(_) => {
                // Rule 3.
                for j in moving(is_main) {
                    out.push(format!("{} while {} moves", name(i), name(j)));
                }
                // Rule 4: where the flipper's side matters, the flipper is at rest.
                if !(allowed(n, state.lift, true, op.cmd) && allowed(n, state.lift, false, op.cmd)) {
                    for j in moving(is_flip) {
                        out.push(format!("{} needs the flipper at rest, but {} is moving it", name(i), name(j)));
                    }
                }
            }
            _ => {}
        }
        match op.cmd {
            Cmd::Lift(to) => state.lift = to,
            Cmd::Flip { up, .. } => state.flip_up = up,
            _ => {}
        }
    }
    out
}
