//! MultiCuber's planner: solution moves -> robot commands, a faithful port
//! of `Planner` in MultiCuber's `robot.py` (the 2018 programs' behaviour).
//!
//! The robot has a lift that raises the cube into a rotator (at level k the
//! rotator turns the k top layers, at level n the whole cube; at 0 the cube
//! sits in the flipper), a flipper that tips the cube about the left-right
//! axis when the lift is at 0 and moves alone otherwise, and the rotator.
//! A turn of U (k layers) is LIFT k + ROT; a turn of D (k layers) is
//! LIFT n-k + ROT, after which the part of the cube with the solver's frame
//! has turned, so the side faces have new names. F, R, L and B are first
//! brought to the top (or B to the bottom) with the flipper and whole-cube
//! turns. `tests/robot_planner.rs` holds this against robot.py on hundreds
//! of random sequences (fixtures written by `scripts/robot_fixtures.py`).
//!
//! Besides the commands, the planner can add up the time they take under a
//! `TimeModel`, without keeping them: that is how thousands of candidate
//! solutions are compared in a few milliseconds.

use super::timing::TimeModel;
use crate::robot::{Turn, FACES};

/// Where the lift is: a level 0..=n, or the scanning height.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lift {
    Level(u8),
    Scan,
}

impl Lift {
    pub fn arg(self) -> String {
        match self {
            Lift::Level(k) => k.to_string(),
            Lift::Scan => "SCAN".into(),
        }
    }

    pub fn parse(s: &str) -> Option<Lift> {
        if s.eq_ignore_ascii_case("SCAN") {
            Some(Lift::Scan)
        } else {
            s.parse().ok().map(Lift::Level)
        }
    }
}

/// How the cube turned in the robot during a command (whole, or the part with the frame).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Remap {
    None,
    X,
    XPrime,
    Y,
    YPrime,
    Y2,
}

impl Remap {
    pub fn name(self) -> &'static str {
        match self {
            Remap::None => "",
            Remap::X => "x",
            Remap::XPrime => "x'",
            Remap::Y => "y",
            Remap::YPrime => "y'",
            Remap::Y2 => "y2",
        }
    }
}

/// One line of the firmware's protocol.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cmd {
    Lift(Lift),
    /// `normal`: the scan's forced normal acceleration (`FLIP UP NORMAL`).
    Flip { up: bool, normal: bool },
    Rot(i8),
    State,
    Enable,
    Disable,
}

/// A picture taken after a command: which face it shows and how far to turn it clockwise.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Photo {
    pub index: u8,
    pub face: char,
    pub rotate: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Op {
    pub cmd: Cmd,
    pub remap: Remap,
    pub photo: Option<Photo>,
    /// The solution move this command turns (the rotator turn that does it).
    pub mv: Option<u16>,
    /// What the lift was before: the time of a LIFT depends on where it starts,
    /// and a FLIP carries the cube only from level 0.
    pub lift_before: Lift,
}

impl Op {
    pub fn line(&self) -> String {
        match self.cmd {
            Cmd::Lift(l) => format!("LIFT {}", l.arg()),
            Cmd::Flip { up, normal } => format!("FLIP {}{}", if up { "UP" } else { "DOWN" }, if normal { " NORMAL" } else { "" }),
            Cmd::Rot(t) => format!("ROT {}", t),
            Cmd::State => "STATE".into(),
            Cmd::Enable => "ENABLE".into(),
            Cmd::Disable => "DISABLE".into(),
        }
    }

    /// `cmd` and `arg` as the API shows them.
    pub fn parts(&self) -> (&'static str, String) {
        let line = self.line();
        let (cmd, arg) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let cmd: &'static str = match cmd {
            "LIFT" => "LIFT",
            "FLIP" => "FLIP",
            "ROT" => "ROT",
            "STATE" => "STATE",
            "ENABLE" => "ENABLE",
            _ => "DISABLE",
        };
        (cmd, arg.to_string())
    }

    /// A FLIP that tips the cube (the lift is down).
    pub fn carries(&self) -> bool {
        matches!(self.cmd, Cmd::Flip { .. }) && self.lift_before == Lift::Level(0)
    }
}

// Faces are numbered as in anycube: U R F D L B.
const U: u8 = 0;
const R: u8 = 1;
const F: u8 = 2;
const D: u8 = 3;
const L: u8 = 4;
const B: u8 = 5;

/// How the robot's positions move under a whole-cube turn (position -> position).
/// x: the front goes to the top.
pub const PERM_X: [u8; 6] = [B, R, U, F, L, D];
/// y: the front goes to the left.
pub const PERM_Y: [u8; 6] = [U, F, L, D, B, R];
/// z: the top goes to the right.
pub const PERM_Z: [u8; 6] = [R, D, F, L, U, B];

fn power(perm: &[u8; 6], k: usize) -> [u8; 6] {
    let mut out = [0, 1, 2, 3, 4, 5];
    for _ in 0..k % 4 {
        out = std::array::from_fn(|p| perm[out[p] as usize]);
    }
    out
}

pub(crate) fn turn_perm(perm: &[u8; 6], turns: i8) -> [u8; 6] {
    power(perm, match turns {
        1 => 1,
        -1 => 3,
        _ => 2,
    })
}

/// How B is brought to the rotator: `Legacy` copies the 2018 programs (B
/// goes to the bottom for the first two moves of a run, to the top after
/// that); `Flipper` chooses by where the flipper is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BRule {
    Legacy,
    Flipper,
}

/// The robot's state as the planner tracks it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct RobotState {
    pub lift: Lift,
    pub flip_up: bool,
}

impl Default for RobotState {
    /// Where the firmware assumes it is at power-up: lift down, flipper up.
    fn default() -> Self {
        RobotState { lift: Lift::Level(0), flip_up: true }
    }
}

pub struct Planner<'a> {
    pub n: u8,
    pub b_rule: BRule,
    pub lift: Lift,
    pub flip_up: bool,
    /// The solver's face name -> robot position.
    pub frame: [u8; 6],
    /// The commands, when they are kept.
    pub ops: Vec<Op>,
    keep: bool,
    timing: Option<&'a TimeModel>,
    /// Robot time of the commands so far, under the time model.
    pub ms: f64,
    pub count: usize,
}

impl<'a> Planner<'a> {
    pub fn new(n: usize, b_rule: BRule) -> Planner<'a> {
        Planner {
            n: n as u8,
            b_rule,
            lift: Lift::Level(0),
            flip_up: true,
            frame: [0, 1, 2, 3, 4, 5],
            ops: Vec::new(),
            keep: true,
            timing: None,
            ms: 0.0,
            count: 0,
        }
    }

    /// A planner that only adds up the time (no commands kept).
    pub fn costing(n: usize, b_rule: BRule, timing: &'a TimeModel) -> Planner<'a> {
        Planner { keep: false, timing: Some(timing), ..Planner::new(n, b_rule) }
    }

    pub fn with_timing(mut self, timing: &'a TimeModel) -> Planner<'a> {
        self.timing = Some(timing);
        self
    }

    pub fn from_state(mut self, s: RobotState) -> Planner<'a> {
        self.lift = s.lift;
        self.flip_up = s.flip_up;
        self
    }

    pub fn state(&self) -> RobotState {
        RobotState { lift: self.lift, flip_up: self.flip_up }
    }

    // -- primitives (they keep the state as the firmware does)
    fn emit(&mut self, cmd: Cmd, remap: Remap, mv: Option<u16>, lift_before: Lift) {
        let op = Op { cmd, remap, photo: None, mv, lift_before };
        if let Some(t) = self.timing {
            self.ms += t.op_ms(&op);
        }
        self.count += 1;
        if self.keep {
            self.ops.push(op);
        }
    }

    pub fn lift_to(&mut self, level: Lift) {
        if level != self.lift {
            let before = self.lift;
            self.lift = level;
            self.emit(Cmd::Lift(level), Remap::None, None, before);
        }
    }

    pub fn flip(&mut self, up: bool, normal: bool) {
        if up == self.flip_up {
            return;
        }
        let carry = self.lift == Lift::Level(0);
        self.flip_up = up;
        let remap = if carry { if up { Remap::X } else { Remap::XPrime } } else { Remap::None };
        self.emit(Cmd::Flip { up, normal }, remap, None, self.lift);
        if carry {
            self.cube_turned(&PERM_X, if up { 1 } else { -1 });
        }
    }

    /// Moves the flipper without the cube: with the lift down it would tip
    /// the cube, so the lift first goes to the scanning height.
    pub fn flip_empty(&mut self, up: bool) {
        if up != self.flip_up && self.lift == Lift::Level(0) {
            self.lift_to(Lift::Scan);
        }
        self.flip(up, false);
    }

    pub fn rot(&mut self, turns: i8, frame_turns: bool, mv: Option<u16>) {
        let mut remap = Remap::None;
        if frame_turns {
            remap = match turns {
                1 => Remap::Y,
                -1 => Remap::YPrime,
                _ => Remap::Y2,
            };
            self.cube_turned(&PERM_Y, turns);
        }
        self.emit(Cmd::Rot(turns), remap, mv, self.lift);
    }

    fn cube_turned(&mut self, perm: &[u8; 6], turns: i8) {
        let p = turn_perm(perm, turns);
        self.frame = std::array::from_fn(|f| p[self.frame[f] as usize]);
    }

    // -- sequences
    /// A new solution: its face names are where the faces are now.
    pub fn start_run(&mut self) {
        self.frame = [0, 1, 2, 3, 4, 5];
    }

    pub fn turn(&mut self, mv: Turn, index: usize) {
        let n = self.n;
        if let Turn::Whole { axis, turns } = mv {
            // The solver turned the cube in its head: rename, nothing moves.
            let perm = [&PERM_X, &PERM_Y, &PERM_Z][axis as usize];
            let back = turn_perm(perm, if turns != 2 { -turns } else { 2 });
            let old = self.frame;
            self.frame = std::array::from_fn(|f| old[back[f] as usize]);
            return;
        }
        let Turn::Layers { face, depth, turns } = mv else { unreachable!() };
        let pos = self.frame[face as usize];
        let mut side = U;
        if pos == D {
            side = D;
        } else if pos == F {
            self.front_to_top();
        } else if pos == R || pos == L {
            self.lift_to(Lift::Level(n));
            self.rot(if pos == R { 1 } else { -1 }, true, None);
            self.front_to_top();
        } else if pos == B {
            let bottom = match self.b_rule {
                BRule::Legacy => index < 2,
                BRule::Flipper => !self.flip_up,
            };
            if bottom {
                self.front_to_top(); // B goes to the bottom
                side = D;
            } else {
                self.flip_empty(true);
                self.lift_to(Lift::Level(0));
                self.flip(false, false); // B goes to the top
            }
        }
        let level = if side == U { depth } else { n - depth };
        self.lift_to(Lift::Level(level));
        self.rot(turns, side == D || level == n, Some(index as u16));
    }

    fn front_to_top(&mut self) {
        self.flip_empty(false);
        self.lift_to(Lift::Level(0));
        self.flip(true, false);
    }

    /// The commands of a whole solution, with the 2018 finish: lift down,
    /// tip the cube back and forth.
    pub fn solution(&mut self, moves: &[Turn]) {
        self.start_run();
        for (i, &mv) in moves.iter().enumerate() {
            self.turn(mv, i);
        }
        self.lift_to(Lift::Level(0));
        self.flip(false, false);
        self.flip(true, false);
    }

    /// The six pictures, as in 2018. Each command with a photo is followed by a picture.
    pub fn scan(&mut self, scan_flip_normal: bool) {
        let shots = SCAN_PHOTOS;
        self.flip(false, false);
        self.photo(0, shots[0]);
        for (i, &shot) in shots.iter().enumerate().take(4).skip(1) {
            self.lift_to(Lift::Scan);
            self.flip(true, scan_flip_normal);
            self.lift_to(Lift::Level(0));
            self.flip(false, false);
            self.photo(i, shot);
        }
        for (i, turns) in [(4, -1), (5, 2)] {
            self.lift_to(Lift::Level(self.n));
            self.rot(turns, true, None);
            self.lift_to(Lift::Level(0));
            self.flip(true, false);
            self.flip(false, false);
            self.photo(i, shots[i]);
        }
    }

    fn photo(&mut self, index: usize, (face, rotate): (char, u16)) {
        let photo = Photo { index: index as u8, face, rotate };
        if !self.keep {
            return;
        }
        match self.ops.last_mut() {
            Some(last) => last.photo = Some(photo),
            None => {
                self.ops.push(Op { cmd: Cmd::State, remap: Remap::None, photo: Some(photo), mv: None, lift_before: self.lift });
                self.count += 1;
            }
        }
    }

    pub fn park(&mut self) {
        self.flip(true, false);
    }
}

/// The 2018 scan: which face each picture shows and the clockwise turn that makes it upright.
pub const SCAN_PHOTOS: [(char, u16); 6] = [('U', 270), ('R', 180), ('D', 90), ('L', 0), ('B', 0), ('F', 0)];

/// The faces in their order, for frames.
pub fn face_char(f: u8) -> char {
    FACES[f as usize] as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_turn_perms_are_quarter_turns() {
        for p in [PERM_X, PERM_Y, PERM_Z] {
            assert_eq!(power(&p, 4), [0, 1, 2, 3, 4, 5]);
            assert_ne!(power(&p, 2), [0, 1, 2, 3, 4, 5]);
        }
    }
}
