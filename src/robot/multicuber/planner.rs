//! MultiCuber's commands and what they do to the robot's state: the lines
//! of the firmware's protocol, a builder that keeps the lift, the flipper
//! and where the cube's faces are as commands are added (the primitives of
//! `Planner` in MultiCuber's `robot.py`, ported faithfully), and the rule
//! that says which command may follow in which state (`allowed`).
//!
//! The robot has a lift that raises the cube into a rotator, a flipper
//! that tips the cube about the left-right axis when the lift is at 0 and
//! moves alone otherwise, and the rotator.
//!
//! The lift decides how much of the cube the rotator turns. At 0, its
//! rest, the cube sits in the flipper and the rotator holds nothing (no
//! ROT is ever sent there). Level 1, the lowest of the raised positions
//! (370 steps on a 3x3x3), brings the top layer alone into the rotator: a
//! ROT there turns the top face. Each level higher (some 50 to 70 steps)
//! brings in one more layer: level 2 turns the top two layers, level k the
//! top k, and at level n the whole cube is in the rotator and turns as one.
//! The scanning height is below level 1: the cube is clear of the flipper
//! there and of the rotator too.
//!
//! So a turn of U (k layers) is LIFT k + ROT. A turn of D (k layers) is
//! LIFT n-k + ROT: the top n-k layers turn over the bottom k, which is the
//! same change to the cube as D turned the same way and the whole cube
//! turned with it (y), so afterwards the side faces have new names (the
//! command's remap). `physical` has the same mechanics on stickers and
//! `sim` runs them; `tests/robot_plans.rs` holds all three against the
//! command lists that robot.py itself made (`the_lift_decides_how_many_layers_turn`).
//!
//! Which commands, in which order, is decided in `optimize` (the planner
//! that drives the robot). The 2018 rules live on in `legacy`, only as the
//! reference the tests hold this against.

use super::timing::TimeModel;
use crate::robot::FACES;

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
    /// Overlap study only (`overlap`): started together with the command
    /// before it instead of after it. Never set for the firmware of today,
    /// which runs one command at a time.
    pub joined: bool,
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
    last: Option<Op>,
}

impl<'a> Planner<'a> {
    pub fn new(n: usize) -> Planner<'a> {
        Planner {
            n: n as u8,
            lift: Lift::Level(0),
            flip_up: true,
            frame: [0, 1, 2, 3, 4, 5],
            ops: Vec::new(),
            keep: true,
            timing: None,
            ms: 0.0,
            count: 0,
            last: None,
        }
    }

    /// A planner that only adds up the time (no commands kept).
    pub fn costing(n: usize, timing: &'a TimeModel) -> Planner<'a> {
        Planner { keep: false, timing: Some(timing), ..Planner::new(n) }
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
        let op = Op { cmd, remap, photo: None, mv, lift_before, joined: false };
        if let Some(t) = self.timing {
            self.ms += t.op_ms(&op);
        }
        self.count += 1;
        self.last = Some(op);
        if self.keep {
            self.ops.push(op);
        }
    }

    /// Overlap study (`overlap`): the last two commands, a lift move or a
    /// rotator turn and a move of the flipper alone, are started together.
    /// The time counted for them becomes that of the pair.
    pub fn join(&mut self, before: Op) {
        let Some(mut last) = self.last else { return };
        last.joined = true;
        if let Some(t) = self.timing {
            let pair = super::overlap::pair(t, &before, &last).expect("only pairs that may overlap are joined");
            self.ms += pair.end - t.op_ms(&before) - t.op_ms(&last);
        }
        self.last = Some(last);
        if self.keep {
            *self.ops.last_mut().expect("the command was kept") = last;
        }
    }

    /// The command added last.
    pub fn last(&self) -> Option<Op> {
        self.last
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

    pub(crate) fn cube_turned(&mut self, perm: &[u8; 6], turns: i8) {
        let p = turn_perm(perm, turns);
        self.frame = std::array::from_fn(|f| p[self.frame[f] as usize]);
    }

    /// A new solution: its face names are where the faces are now.
    pub fn start_run(&mut self) {
        self.frame = [0, 1, 2, 3, 4, 5];
    }

    /// The solver turned the cube in its head (x, y, z in a solution): the
    /// faces are renamed, nothing moves.
    pub fn rename(&mut self, axis: u8, turns: i8) {
        self.frame = renamed(self.frame, axis, turns);
    }

    /// Marks the last command as followed by a picture (a `STATE` stands in when there is none).
    pub fn photo(&mut self, index: usize, face: char, rotate: u16) {
        let photo = Photo { index: index as u8, face, rotate };
        if !self.keep {
            return;
        }
        match self.ops.last_mut() {
            Some(last) => last.photo = Some(photo),
            None => {
                self.ops.push(Op { cmd: Cmd::State, remap: Remap::None, photo: Some(photo), mv: None, lift_before: self.lift, joined: false });
                self.count += 1;
            }
        }
    }
}

/// A frame after the solver's whole-cube turn (a rename).
pub fn renamed(frame: [u8; 6], axis: u8, turns: i8) -> [u8; 6] {
    let perm = [&PERM_X, &PERM_Y, &PERM_Z][axis as usize];
    let back = turn_perm(perm, if turns != 2 { -turns } else { 2 });
    std::array::from_fn(|f| frame[back[f] as usize])
}

/// The robot position the camera looks at when the lift is down and the flipper DOWN.
pub const CAMERA: u8 = F;

/// Whether a command may be sent with the lift and the flipper where they
/// are: exactly the cases in which robot.py's planner (the 2018 programs'
/// behaviour) sends it, for scans and solves from any state, and no other.
/// The planner makes only these, the simulated robot and the tests count
/// anything else as a violation, and `tests/robot_plans.rs` holds this rule
/// against what the 2018 planner really produces.
///
/// - LIFT: between 0 and any level, between the turning levels, and from
///   the top level n back to 0 with the flipper DOWN; the scanning height
///   only from 0 and back to 0.
/// - FLIP: with the lift at 0 (it tips the cube), at the scanning height or
///   at a turning level (the cube is off it, it moves alone); at level n
///   only DOWN.
/// - ROT: never with the lift at 0 or at the scanning height; a half turn
///   of the whole cube (level n) only with the flipper DOWN.
pub fn allowed(n: u8, lift: Lift, flip_up: bool, cmd: Cmd) -> bool {
    match (cmd, lift) {
        (Cmd::Lift(to), _) if to == lift => false,
        (Cmd::Lift(Lift::Scan), from) => from == Lift::Level(0),
        (Cmd::Lift(Lift::Level(to)), Lift::Scan) => to == 0,
        (Cmd::Lift(Lift::Level(to)), Lift::Level(from)) => to <= n && from <= n && (from < n || (to == 0 && !flip_up)),
        (Cmd::Flip { up, .. }, Lift::Level(k)) => up != flip_up && k <= n && (k < n || !up),
        (Cmd::Flip { up, .. }, Lift::Scan) => up != flip_up,
        (Cmd::Rot(t), Lift::Level(k)) => matches!(t, 1 | -1 | 2) && k >= 1 && k <= n && (k < n || t != 2 || !flip_up),
        (Cmd::Rot(_), Lift::Scan) => false,
        (Cmd::State | Cmd::Enable | Cmd::Disable, _) => true,
    }
}

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
