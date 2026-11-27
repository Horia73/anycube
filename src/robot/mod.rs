//! Cube robots: what anycube needs to know of one, and the robots it knows.
//!
//! A robot is reached through a `Driver` (one per kind of robot: what it is
//! called, the cube sizes it takes, the ports it may be on, how to connect)
//! and, once connected, is a `Robot`: it says how it scans, turns a
//! solution into its own commands with a time for each (`plan`), runs them
//! with a call after each (`execute`) and can be stopped at once. The
//! robot-fastest solution is chosen by `fastest` with the robot's own
//! estimate of a solution's time, and `session` runs scans and solves for
//! the local server's API (robots-spec/API.md).
//!
//! Boards are found as they are plugged in: `detect` lists the serial ports
//! that may carry one and asks each new one once, with the drivers'
//! handshakes, what it is.
//!
//! The first driver is MultiCuber's (`multicuber`). A simulated MultiCuber
//! (`multicuber::sim`) answers like the firmware and takes as long, or a
//! given fraction of it, so all of this can run without the hardware.

pub mod detect;
pub mod fastest;
pub mod multicuber;
pub mod physical;
pub mod session;
pub mod solution_link;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Face letters in anycube's order.
pub const FACES: [u8; 6] = *b"URFDLB";

/// A move as robots see it: some layers from a face, or the whole cube.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Turn {
    /// `depth` layers from `face` (0..6 in URFDLB order), `turns` 1, -1 or 2.
    Layers { face: u8, depth: u8, turns: i8 },
    /// x (0), y (1) or z (2): the solver turned the cube in its head.
    Whole { axis: u8, turns: i8 },
}

impl Turn {
    /// anycube's notation: `R`, `R'`, `R2`, `Rw` (2 layers), `3Rw` (3 layers), `x`, `y`, `z`.
    pub fn parse(token: &str, n: usize) -> Result<Turn, String> {
        let bad = || format!("not a move: '{}'", token);
        let t = token.trim();
        let (body, turns) = if let Some(b) = t.strip_suffix('\'') {
            (b, -1)
        } else if let Some(b) = t.strip_suffix('2').filter(|b| !b.is_empty() && !b.bytes().all(|c| c.is_ascii_digit())) {
            (b, 2)
        } else {
            (t, 1)
        };
        if let Some(axis) = ["x", "y", "z"].iter().position(|&a| a == body) {
            return Ok(Turn::Whole { axis: axis as u8, turns });
        }
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
        let rest = &body[digits..];
        let (face_ch, wide) = match rest.as_bytes() {
            [f] => (*f, false),
            [f, b'w'] => (*f, true),
            _ => return Err(bad()),
        };
        let face = FACES.iter().position(|&c| c == face_ch).ok_or_else(bad)? as u8;
        let depth = if digits > 0 {
            if !wide {
                return Err(format!("'{}': use {}{}w for {} layers", token, &body[..digits], face_ch as char, &body[..digits]));
            }
            body[..digits].parse::<usize>().map_err(|_| bad())?
        } else if wide {
            2
        } else {
            1
        };
        if depth < 1 || depth >= n {
            return Err(format!("'{}': a {}x{}x{} turns 1..{} layers from a face", token, n, n, n, n - 1));
        }
        Ok(Turn::Layers { face, depth: depth as u8, turns })
    }

    pub fn parse_all(text: &str, n: usize) -> Result<Vec<Turn>, String> {
        text.split_whitespace().map(|t| Turn::parse(t, n)).collect()
    }

    /// From anycube's move numbering (`cuben.rs`): `(face + 6 * (layers - 1)) * 3 + power`.
    pub fn from_index(m: usize) -> Turn {
        let (face, layers, power) = (m / 3 % 6, m / 18 + 1, m % 3);
        Turn::Layers { face: face as u8, depth: layers as u8, turns: [1, 2, -1][power] }
    }

    pub fn name(&self) -> String {
        let suffix = |t: i8| match t {
            1 => "",
            -1 => "'",
            _ => "2",
        };
        match *self {
            Turn::Whole { axis, turns } => format!("{}{}", ["x", "y", "z"][axis as usize], suffix(turns)),
            Turn::Layers { face, depth, turns } => {
                let f = FACES[face as usize] as char;
                let body = match depth {
                    1 => f.to_string(),
                    2 => format!("{}w", f),
                    k => format!("{}{}w", k, f),
                };
                body + suffix(turns)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RobotError {
    pub code: &'static str,
    pub message: String,
}

impl RobotError {
    pub fn new(code: &'static str, message: impl Into<String>) -> RobotError {
        RobotError { code, message: message.into() }
    }
}

impl std::fmt::Display for RobotError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// A serial port that may carry a robot's board, with what USB says of the board.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct PortId {
    pub path: String,
    /// Vendor and product numbers.
    pub usb: Option<(u16, u16)>,
    pub serial: Option<String>,
}

/// What a driver's handshake on a port found.
#[derive(Clone, Debug, PartialEq)]
pub enum Probe {
    /// The driver's robot answered: what it said of itself.
    Robot(serde_json::Value),
    /// A board that does not answer as this robot.
    NotThis,
    /// The port could not be opened: another program holds it.
    Busy,
}

/// A kind of robot.
pub trait Driver: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// Cube sizes the robot takes.
    fn sizes(&self) -> Vec<usize>;
    /// Whether a port may carry this robot, going by its name and USB numbers.
    fn claims(&self, _port: &PortId) -> bool {
        true
    }
    /// Asks the board on a port what it is, with the handshake only: nothing moves.
    fn probe(&self, port: &str) -> Probe;
    /// Opens a port and checks that the robot answers.
    /// `size`: the cube that is in it, when the robot should be told.
    fn connect(&self, port: &str, size: Option<usize>, opts: &ConnectOptions) -> Result<Box<dyn Robot>, RobotError>;
}

/// What a connection is given: where it may keep what it learns, whether it is simulated.
#[derive(Clone, Debug, Default)]
pub struct ConnectOptions {
    /// Directory for the robot's learned timings (anycube's settings directory).
    pub settings_dir: Option<std::path::PathBuf>,
    /// Simulate the robot, its motions taking this fraction of their real time (0 = no waiting).
    pub sim_scale: Option<f64>,
}

/// One planned robot command.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOp {
    pub cmd: String,
    pub arg: String,
    pub est_ms: f64,
    /// The solution move this command turns.
    pub mv: Option<usize>,
    /// A picture to take once it is done.
    pub photo: Option<PhotoSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhotoSpec {
    pub index: usize,
    pub face: char,
    /// Degrees clockwise, 0/90/180/270.
    pub rotate: u16,
}

/// Commands for one run, with their times.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub ops: Vec<PlannedOp>,
    pub eta_ms: f64,
}

/// What `execute` reports as it goes.
#[derive(Clone, Debug)]
pub enum Progress {
    State(String),
    /// Command `i` has started moving (as far as the computer can tell).
    Started { i: usize },
    /// Command `i` is done; it took `actual_ms`.
    Done { i: usize, actual_ms: f64 },
    /// The controller completed logical move i.
    MoveDone { i: usize, actual_ms: f64 },
    /// Command `i` was followed by a picture: the call returns once it is taken
    /// (an error stops the run).
    Photo { i: usize, photo: PhotoSpec },
    /// A controller-labelled frame, optionally carrying the controller camera PNG.
    Frame { i: usize, photo: PhotoSpec, png: Option<String> },
}

/// Set from anywhere to stop a run as soon as possible.
#[derive(Clone, Default)]
pub struct StopFlag(Arc<AtomicBool>);

impl StopFlag {
    pub fn stop(&self) {
        self.0.store(true, Ordering::SeqCst)
    }
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn clear(&self) {
        self.0.store(false, Ordering::SeqCst)
    }
}

/// A connected robot.
pub trait Robot: Send {
    fn driver_id(&self) -> &'static str;
    fn port(&self) -> String;
    fn size(&self) -> usize;
    /// Hold a size-independent viewing pose, read one measurement, and undo the hold.
    fn measure_size(&mut self, _stop: &StopFlag, _read: &mut dyn FnMut() -> Result<usize, RobotError>) -> Result<usize, RobotError> {
        Err(RobotError::new("unsupported", "this controller has no size-photo pose"))
    }
    /// Apply the size established before scanning. Fixed-size controllers advertise it at handshake.
    fn prepare_size(&mut self, n: usize) -> Result<(), RobotError> {
        if self.size() == n { Ok(()) } else { Err(RobotError::new("bad_size", "cube differs from controller size")) }
    }
    /// What the robot said about itself at the handshake.
    fn info(&self) -> serde_json::Value;
    /// Asks the robot where it is, before planning.
    fn sync(&mut self) -> Result<(), RobotError>;
    /// The commands of a scan, from where the robot is now; the pictures are on them.
    fn plan_scan(&self) -> Plan;
    /// A host-planned return from the completed scan, computed before the first motion.
    fn scan_return_plan(&self, _plan: &Plan) -> Option<Plan> { None }
    /// The pictures of a scan from where the robot is now, in order.
    fn scan_photos(&self) -> Vec<PhotoSpec> {
        self.plan_scan().ops.iter().filter_map(|o| o.photo).collect()
    }
    /// The commands of a solution, from where the robot is now.
    fn plan_solution(&self, moves: &[Turn]) -> Plan;
    /// Validate a solution against this controller's capabilities before execution.
    fn check_solution(&self, _moves: &[Turn]) -> Result<(), RobotError> {
        Ok(())
    }
    /// Use the normal solver when the controller owns the motion plan.
    fn controller_planned(&self) -> bool {
        false
    }
    /// The robot time (ms) of a solution from where the robot is now, quickly: candidates are compared by it.
    /// It stands on its own: it may be used while the robot does something else.
    fn estimator(&self) -> Box<dyn Fn(&[Turn]) -> f64 + Send>;
    /// The name of the planner, for the `plan` event.
    fn planner_name(&self) -> &'static str {
        "optimized"
    }
    /// Says, when asked, whether the robot is still plugged in (asked from another thread while it runs).
    fn presence(&self) -> Arc<dyn Fn() -> bool + Send + Sync> {
        Arc::new(|| true)
    }
    fn connection_closed(&self) -> bool {
        false
    }
    /// Runs a plan. `progress` is called as the commands go; an error from it stops the run.
    fn execute(&mut self, plan: &Plan, stop: &StopFlag, progress: &mut dyn FnMut(Progress) -> Result<(), RobotError>) -> Result<(), RobotError>;
    /// Motors off, now (when no run is going on; a run stops through its `StopFlag`).
    fn emergency_stop(&mut self);
    /// Brings the robot to where it can be switched off, if it is not there.
    fn park(&mut self) -> Result<(), RobotError>;
    /// What the camera would see now, for a robot that is simulated: the
    /// stickers of the face in front of it, row by row, the right way up
    /// (the `sim` of a `photo` event, so that a page can be tried with no
    /// robot and no cube). A real robot has none.
    fn camera(&self) -> Option<Arc<dyn Fn() -> String + Send + Sync>> {
        None
    }
    /// A line for the log after a run, if the robot has something to say (the simulator does).
    fn after_run_note(&mut self) -> Option<String> {
        None
    }
    fn close(&mut self);
    /// For tests that look inside a driver (the simulator).
    fn as_any(&self) -> &dyn std::any::Any;
}

/// The robots anycube knows.
pub fn drivers() -> Vec<Box<dyn Driver>> {
    vec![Box::new(multicuber::MultiCuberDriver), Box::new(solution_link::SolutionDriver)]
}
