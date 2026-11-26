//! A simulated MultiCuber: a transport that answers like firmware 2.2 and
//! takes as long as the robot does (the time model's estimates, times a
//! scale; 0 = no waiting), with a cube on the sticker model inside it.
//!
//! Like the Mega it runs one command at a time and keeps what arrives
//! meanwhile in a 64-byte receive buffer: more than that in flight is lost
//! and counted in `overflows`, so a test sees if the computer sends too far
//! ahead. It also counts steps the robot must not make (`violations`).
//!
//! `anycube app` uses it when `ANYCUBE_ROBOT_SIM` is set (to the time
//! scale: `1` real time, `0.2` five times faster, `0` instant) or with
//! `--robot-sim[=scale]`: MultiCuber then lists the port `sim`.

use super::link::Transport;
use super::planner::Lift;
use super::timing::{Cal, TimeModel};
use crate::robot::physical::PhysicalCube;
use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The Mega's serial receive buffer holds 63 bytes (64, one kept free).
pub const RX_BUFFER: usize = 63;

pub struct SimState {
    pub cube: PhysicalCube,
    pub enabled: bool,
    pub size: usize,
    pub log: Vec<String>,
    pub overflows: usize,
    pub violations: Vec<String>,
    /// Bytes received and not yet taken by the command handler, with the
    /// time they arrive (the serial line's latency).
    rx: VecDeque<(Instant, u8)>,
    tx: VecDeque<(Instant, u8)>,
    closed: bool,
    /// Lines the handler is working through (taken from `rx`).
    busy: bool,
}

/// The shared inside of a simulated robot: tests look at it.
#[derive(Clone)]
pub struct SimRobot {
    pub state: Arc<(Mutex<SimState>, Condvar)>,
    pub scale: f64,
}

impl SimRobot {
    /// A robot with a cube of size `n` scrambled from `seed` (0: solved).
    pub fn new(n: usize, seed: u64, scale: f64) -> SimRobot {
        let mut cube = PhysicalCube::new(n);
        if seed != 0 {
            cube.scramble(seed, 40 * n);
        }
        let state = SimState {
            cube,
            enabled: false,
            size: n,
            log: Vec::new(),
            overflows: 0,
            violations: Vec::new(),
            rx: VecDeque::new(),
            tx: VecDeque::new(),
            closed: false,
            busy: false,
        };
        let robot = SimRobot { state: Arc::new((Mutex::new(state), Condvar::new())), scale };
        let inner = robot.clone();
        std::thread::spawn(move || inner.firmware());
        // The banner of a Mega that has just started.
        robot.reply("# MULTICUBER 2.2.0 ready (send HELLO)");
        robot
    }

    pub fn transport(&self) -> Box<dyn Transport> {
        Box::new(SimTransport { robot: self.clone() })
    }

    /// One way of the serial line and the USB in between (half the measured round trip).
    fn latency(&self) -> Duration {
        Duration::from_secs_f64(0.006 * self.scale.max(0.0))
    }

    fn reply(&self, line: &str) {
        let (m, cv) = &*self.state;
        let mut s = m.lock().unwrap();
        let at = Instant::now() + self.latency();
        s.tx.extend(line.bytes().chain([b'\n']).map(|b| (at, b)));
        cv.notify_all();
    }

    fn wait(&self, ms: f64) {
        let ms = ms * self.scale;
        if ms > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(ms / 1000.0));
        }
    }

    /// The command handler: one line at a time, as the firmware's loop().
    fn firmware(&self) {
        let (m, cv) = &*self.state;
        loop {
            let line = {
                let mut s = m.lock().unwrap();
                loop {
                    if s.closed {
                        return;
                    }
                    if let Some(end) = s.rx.iter().position(|&(_, b)| b == b'\n') {
                        let at = s.rx[end].0;
                        let now = Instant::now();
                        if at <= now {
                            let bytes: Vec<u8> = s.rx.drain(..=end).map(|(_, b)| b).collect();
                            s.busy = true;
                            break String::from_utf8_lossy(&bytes).trim().to_ascii_uppercase();
                        }
                        s = cv.wait_timeout(s, at - now).unwrap().0;
                        continue;
                    }
                    s = cv.wait(s).unwrap();
                }
            };
            let (answer, ms) = self.handle(&line);
            self.wait(ms);
            {
                let mut s = m.lock().unwrap();
                s.busy = false;
                s.log.push(line);
            }
            for l in answer {
                self.reply(&l);
            }
        }
    }

    /// The answer lines to a command and how long it takes (ms of robot time).
    fn handle(&self, line: &str) -> (Vec<String>, f64) {
        let (m, _) = &*self.state;
        let mut s = m.lock().unwrap();
        let w: Vec<&str> = line.split_whitespace().collect();
        let n = s.size;
        let cal = Cal::default_for(n);
        let model = TimeModel::new(cal.clone());
        let height = |l: Lift| cal.height(l);
        let ok = |t: String| (vec![format!("OK {}", t)], 0.0);
        let Some(&c) = w.first() else { return (vec![], 0.0) };
        let motion = matches!(c, "LIFT" | "FLIP" | "ROT");
        if motion && !s.enabled {
            return (vec!["ERR DISABLED send ENABLE first".into()], 0.0);
        }
        match c {
            "HELLO" | "VERSION" => ok(format!(
                "MULTICUBER fw=2.2.0-sim proto=1 backend=SIM size={} ms=8 speed=MEDIUM rpm=600 maxrate=40000",
                n
            )),
            "PING" => ok("PONG".into()),
            "STATE" => {
                let lift = height(s.cube.lift);
                let flip = if s.cube.flip_up { "UP" } else { "DOWN" };
                ok(format!(
                    "size={} ms=8 speed=MEDIUM lift={} flip={} enabled={} rpm=600 idle=10 fan={} fanmode=AUTO",
                    n, lift, flip, s.enabled as u8, s.enabled as u8
                ))
            }
            "CAL" if w.len() == 1 => {
                let lifts: Vec<String> = cal.lift[1..].iter().map(|h| h.to_string()).collect();
                let half = if n == 5 { -1 } else { 1 };
                (
                    vec![
                        format!("# n={} lift={} scan={} flip={} rotq={} roto={} half={}", n, lifts.join(","), cal.scan, cal.flip, cal.rotq, cal.roto, half),
                        format!("OK CAL {} DEFAULT", n),
                    ],
                    0.0,
                )
            }
            "SIZE" => match w.get(1).and_then(|v| v.parse::<usize>().ok()) {
                Some(k @ 3..=5) => {
                    if k != s.size {
                        s.size = k;
                        s.cube = PhysicalCube::new(k);
                    }
                    ok(format!("SIZE {}", k))
                }
                Some(k @ 6..=7) => (vec![format!("ERR NOCAL no table for {}", k)], 0.0),
                _ => (vec!["ERR RANGE SIZE 3..7".into()], 0.0),
            },
            "SPEED" => ok(format!("SPEED {}", w.get(1).unwrap_or(&"MEDIUM"))),
            "MICROSTEPS" => ok("MICROSTEPS 8 rpm=600".into()),
            "IDLE" => ok(format!("IDLE {}", w.get(1).unwrap_or(&"10"))),
            "ENABLE" => {
                let ramp = if s.enabled { 0.0 } else { model.key_ms("ENABLE") };
                s.enabled = true;
                (vec!["OK ENABLE".into()], ramp)
            }
            "DISABLE" => {
                s.enabled = false;
                ok("DISABLE".into())
            }
            "ZERO" => {
                s.cube.apply_line("ZERO");
                ok("ZERO".into())
            }
            "LIFT" => {
                let to = match w.get(1).copied() {
                    Some("DOWN") => Some(Lift::Level(0)),
                    Some("UP") => Some(Lift::Level(n as u8)),
                    Some(a) => Lift::parse(a).filter(|l| !matches!(l, Lift::Level(k) if *k as usize > n)),
                    None => None,
                };
                let Some(to) = to else { return (vec![format!("ERR RANGE LIFT 0..{}|DOWN|UP|SCAN", n)], 0.0) };
                let from = s.cube.lift;
                let op = super::planner::Op { cmd: super::planner::Cmd::Lift(to), remap: super::planner::Remap::None, photo: None, mv: None, lift_before: from };
                let ms = if from == to { 0.0 } else { model.op_ms(&op) - model.overhead_ms };
                s.cube.apply_line(line);
                (vec![format!("OK LIFT {} {}", w[1], height(to))], ms)
            }
            "FLIP" => {
                let up = match w.get(1).copied() {
                    Some("UP") => true,
                    Some("DOWN") => false,
                    _ => return (vec!["ERR ARG FLIP UP|DOWN [NORMAL|FAST]".into()], 0.0),
                };
                let normal = w.get(2) == Some(&"NORMAL");
                let carry = s.cube.lift == Lift::Level(0);
                let moved = up != s.cube.flip_up;
                let op = super::planner::Op {
                    cmd: super::planner::Cmd::Flip { up, normal },
                    remap: super::planner::Remap::None,
                    photo: None,
                    mv: None,
                    lift_before: s.cube.lift,
                };
                let ms = if moved { model.op_ms(&op) - model.overhead_ms } else { 0.0 };
                s.cube.apply_line(line);
                (vec![format!("OK FLIP {} {}", w[1], if !moved { "STAY" } else if carry { "CARRY" } else { "EMPTY" })], ms)
            }
            "ROT" => {
                let t: i8 = match w.get(1).and_then(|v| v.parse().ok()) {
                    Some(t @ (1 | -1 | 2 | -2)) => t,
                    _ => return (vec!["ERR ARG ROT 1|-1|2".into()], 0.0),
                };
                let ms = model.key_ms(if t.abs() == 2 { "ROT 2" } else { "ROT 1" });
                s.cube.apply_line(line);
                (vec![format!("OK ROT {}", t)], ms)
            }
            _ => (vec![format!("ERR UNKNOWN {} (try HELP)", c)], 0.0),
        }
    }

    /// Steps the robot must not make, seen so far (by the firmware model and the cube).
    pub fn violations(&self) -> Vec<String> {
        let s = self.state.0.lock().unwrap();
        s.violations.iter().chain(s.cube.violations.iter()).cloned().collect()
    }

    pub fn overflows(&self) -> usize {
        self.state.0.lock().unwrap().overflows
    }

    pub fn is_solved(&self) -> bool {
        self.state.0.lock().unwrap().cube.is_solved()
    }

    pub fn facelets(&self) -> String {
        self.state.0.lock().unwrap().cube.facelet_string()
    }

    pub fn log(&self) -> Vec<String> {
        self.state.0.lock().unwrap().log.clone()
    }

    pub fn is_closed(&self) -> bool {
        self.state.0.lock().unwrap().closed
    }

    /// Pulls the plug: from now on nothing is written or answered, as when the cable goes.
    pub fn unplug(&self) {
        self.close();
    }

    pub fn close(&self) {
        let (m, cv) = &*self.state;
        m.lock().unwrap().closed = true;
        cv.notify_all();
    }
}

struct SimTransport {
    robot: SimRobot,
}

impl Transport for SimTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let (m, cv) = &*self.robot.state;
        let mut s = m.lock().unwrap();
        if s.closed {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the simulated robot is closed"));
        }
        let at = Instant::now() + self.robot.latency();
        for &b in data {
            if s.rx.len() >= RX_BUFFER {
                s.overflows += 1; // lost, as on the Mega
            } else {
                s.rx.push_back((at, b));
            }
        }
        cv.notify_all();
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        let (m, cv) = &*self.robot.state;
        let deadline = Instant::now() + timeout;
        let mut s = m.lock().unwrap();
        loop {
            let now = Instant::now();
            let ready = s.tx.iter().take_while(|&&(at, _)| at <= now).count().min(buf.len());
            if ready > 0 {
                for (i, (_, b)) in s.tx.drain(..ready).enumerate() {
                    buf[i] = b;
                }
                return Ok(ready);
            }
            if now >= deadline || s.closed {
                return Ok(0);
            }
            let until = s.tx.front().map_or(deadline, |&(at, _)| at.min(deadline));
            s = cv.wait_timeout(s, until.saturating_duration_since(now).max(Duration::from_micros(200))).unwrap().0;
        }
    }

    fn clear_input(&mut self) {
        self.robot.state.0.lock().unwrap().tx.clear();
    }
}

impl Drop for SimTransport {
    fn drop(&mut self) {
        self.robot.close();
    }
}
