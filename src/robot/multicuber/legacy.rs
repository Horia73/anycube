//! The 2018 planner, as `Planner` in MultiCuber's `robot.py` has it: NOT
//! used to drive the robot. It is kept as the reference of the tests:
//! `tests/robot_planner.rs` holds it against robot.py command for command
//! (which also proves the shared primitives of `planner`), and
//! `tests/robot_plans.rs` takes from it which commands the robot has ever
//! been sent in which state (the rule `planner::allowed`) and the times
//! the planner in `optimize` must beat.
//!
//! F, R, L and B are brought to the top (or B to the bottom) with the
//! flipper and whole-cube turns by fixed rules; a solution ends with the
//! lift down and the cube tipped back and forth; the scan takes its six
//! pictures in a fixed order.

use super::planner::{Lift, Planner};
use crate::robot::Turn;

const U: u8 = 0;
const R: u8 = 1;
const F: u8 = 2;
const D: u8 = 3;
const L: u8 = 4;
const B: u8 = 5;

/// How B is brought to the rotator: `Legacy` copies the 2018 programs (B
/// goes to the bottom for the first two moves of a run, to the top after
/// that); `Flipper` chooses by where the flipper is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BRule {
    Legacy,
    Flipper,
}

/// The 2018 scan: which face each picture shows and the clockwise turn that makes it upright.
pub const SCAN_PHOTOS: [(char, u16); 6] = [('U', 270), ('R', 180), ('D', 90), ('L', 0), ('B', 0), ('F', 0)];

/// The 2018 sequences over the command builder.
pub struct Legacy<'a> {
    pub p: Planner<'a>,
    pub b_rule: BRule,
}

impl<'a> Legacy<'a> {
    pub fn new(p: Planner<'a>, b_rule: BRule) -> Legacy<'a> {
        Legacy { p, b_rule }
    }

    pub fn turn(&mut self, mv: Turn, index: usize) {
        let n = self.p.n;
        let (face, depth, turns) = match mv {
            Turn::Whole { axis, turns } => return self.p.rename(axis, turns),
            Turn::Layers { face, depth, turns } => (face, depth, turns),
        };
        let pos = self.p.frame[face as usize];
        let mut side = U;
        if pos == D {
            side = D;
        } else if pos == F {
            self.front_to_top();
        } else if pos == R || pos == L {
            self.p.lift_to(Lift::Level(n));
            self.p.rot(if pos == R { 1 } else { -1 }, true, None);
            self.front_to_top();
        } else if pos == B {
            let bottom = match self.b_rule {
                BRule::Legacy => index < 2,
                BRule::Flipper => !self.p.flip_up,
            };
            if bottom {
                self.front_to_top(); // B goes to the bottom
                side = D;
            } else {
                self.p.flip_empty(true);
                self.p.lift_to(Lift::Level(0));
                self.p.flip(false, false); // B goes to the top
            }
        }
        let level = if side == U { depth } else { n - depth };
        self.p.lift_to(Lift::Level(level));
        self.p.rot(turns, side == D || level == n, Some(index as u16));
    }

    fn front_to_top(&mut self) {
        self.p.flip_empty(false);
        self.p.lift_to(Lift::Level(0));
        self.p.flip(true, false);
    }

    /// The commands of a whole solution, with the 2018 finish: lift down,
    /// tip the cube back and forth.
    pub fn solution(&mut self, moves: &[Turn]) {
        self.p.start_run();
        for (i, &mv) in moves.iter().enumerate() {
            self.turn(mv, i);
        }
        self.p.lift_to(Lift::Level(0));
        self.p.flip(false, false);
        self.p.flip(true, false);
    }

    /// The six pictures, as in 2018. Each command with a photo is followed by a picture.
    pub fn scan(&mut self, scan_flip_normal: bool) {
        let shots = SCAN_PHOTOS;
        let p = &mut self.p;
        p.flip(false, false);
        p.photo(0, shots[0].0, shots[0].1);
        for (i, &(face, rotate)) in shots.iter().enumerate().take(4).skip(1) {
            p.lift_to(Lift::Scan);
            p.flip(true, scan_flip_normal);
            p.lift_to(Lift::Level(0));
            p.flip(false, false);
            p.photo(i, face, rotate);
        }
        for (i, turns) in [(4, -1), (5, 2)] {
            p.lift_to(Lift::Level(p.n));
            p.rot(turns, true, None);
            p.lift_to(Lift::Level(0));
            p.flip(true, false);
            p.flip(false, false);
            p.photo(i, shots[i].0, shots[i].1);
        }
    }

    pub fn park(&mut self) {
        self.p.flip(true, false);
    }
}
