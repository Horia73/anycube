//! MultiCuber (Arduino Mega + RAMPS, LIFT / FLIP / ROT steppers, firmware 2
//! line protocol at 115200 baud): the driver.
//!
//! Connecting opens the port (which restarts the Mega), waits for its
//! banner and says HELLO (protocol 1). The cube size is the firmware's,
//! unless the caller names one (then `SIZE n`). The speed preset is set to
//! MEDIUM, the one the time model was measured with, and the calibration
//! table (`CAL`) is read for the lift heights the times depend on.
//!
//! Every command blocks in the firmware until its motion is done, then
//! answers. Waiting for each answer before sending the next would leave
//! the robot standing for a serial round trip (~12 ms) after every command,
//! so the next command is sent while one runs, shortly before it is
//! expected to end (by the time model): the firmware finds it in its
//! receive buffer and goes straight on. At most two commands are in flight
//! and never more bytes than the Mega's 64-byte buffer takes; nothing is
//! sent ahead past a picture. A stop, or an error, sends DISABLE at once;
//! the firmware runs it after the command that is moving (and the one
//! queued behind it, if it was already sent).

pub mod link;
pub mod optimize;
pub mod planner;
pub mod sim;
pub mod timing;

use self::link::{Link, Reply, SerialTransport};
use self::planner::{BRule, Cmd, Lift, Op, Planner, Remap, RobotState};
use self::sim::SimRobot;
use self::timing::{Cal, TimeModel};
use super::{ConnectOptions, Driver, PhotoSpec, Plan, PlanMode, PlannedOp, Progress, Robot, RobotError, StopFlag, Turn};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const PROTOCOL: u32 = 1;
pub const BAUD: u32 = 115200;
/// Sizes with a calibration table in firmware 2.2 (config.h DEFAULT_SIZES).
pub const SIZES: [usize; 3] = [3, 4, 5];
/// Commands in flight at once: the one moving and the next.
pub const MAX_IN_FLIGHT: usize = 2;
/// Bytes in flight at most (the Mega keeps 63 in its receive buffer).
pub const MAX_BYTES: usize = 60;
/// The next command goes out this long before the running one should end.
pub const LEAD_MS: f64 = 30.0;

pub struct MultiCuberDriver;

impl Driver for MultiCuberDriver {
    fn id(&self) -> &'static str {
        "multicuber"
    }

    fn name(&self) -> &'static str {
        "MultiCuber"
    }

    fn sizes(&self) -> Vec<usize> {
        SIZES.to_vec()
    }

    fn ports(&self) -> Vec<String> {
        serial_ports()
    }

    fn connect(&self, port: Option<&str>, size: Option<usize>, opts: &ConnectOptions) -> Result<Box<dyn Robot>, RobotError> {
        if let Some(n) = size.filter(|n| !SIZES.contains(n)) {
            return Err(RobotError::new("bad_size", format!("MultiCuber takes the sizes {:?}, not {}", SIZES, n)));
        }
        let sim = port == Some("sim") || (port.is_none() && opts.sim_scale.is_some());
        let (link, port, sim_robot) = if sim {
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
            let robot = SimRobot::new(size.unwrap_or(3), seed, opts.sim_scale.unwrap_or(1.0));
            (Link::new(robot.transport()), "sim".to_string(), Some(robot))
        } else {
            let port = match port {
                Some(p) => p.to_string(),
                None => serial_ports().into_iter().next().ok_or_else(|| RobotError::new("no_port", "no robot port found; is the robot plugged in?"))?,
            };
            (Link::new(Box::new(SerialTransport::open(&port, BAUD)?)), port, None)
        };
        let timing_path = opts.settings_dir.as_ref().map(|d| d.join("robot-multicuber-timing.json"));
        MultiCuber::start(link, port, size, timing_path, sim_robot).map(|r| Box::new(r) as Box<dyn Robot>)
    }
}

/// Serial ports that look like a robot's board, likeliest first.
pub fn serial_ports() -> Vec<String> {
    let Ok(ports) = serialport::available_ports() else { return Vec::new() };
    let mut out: Vec<(u8, String)> = Vec::new();
    for p in ports {
        let name = p.port_name.clone();
        // On a Mac every device is there twice; the callout (cu) one is the one to open.
        if name.starts_with("/dev/tty.") {
            continue;
        }
        let looks = ["/dev/cu.usbmodem", "/dev/cu.usbserial", "/dev/cu.wchusbserial", "/dev/ttyACM", "/dev/ttyUSB", "COM"]
            .iter()
            .any(|prefix| name.starts_with(prefix));
        let rank = match &p.port_type {
            // Arduino (and its clone makers), WCH CH340, FTDI.
            serialport::SerialPortType::UsbPort(u) if matches!(u.vid, 0x2341 | 0x2A03) => 0,
            serialport::SerialPortType::UsbPort(u) if matches!(u.vid, 0x1A86 | 0x0403) => 1,
            serialport::SerialPortType::UsbPort(_) => 2,
            _ if looks => 3,
            _ => continue,
        };
        out.push((rank, name));
    }
    out.sort();
    out.into_iter().map(|(_, n)| n).collect()
}

pub struct MultiCuber {
    link: Link,
    port: String,
    n: usize,
    hello: BTreeMap<String, String>,
    cal: Cal,
    pub timing: TimeModel,
    timing_path: Option<PathBuf>,
    state: RobotState,
    /// False after an error or a stop, until the robot says where it is.
    state_known: bool,
    enabled: bool,
    sim: Option<SimRobot>,
    /// How B goes to the rotator (robot.py's default).
    pub b_rule: BRule,
}

impl MultiCuber {
    /// The handshake and set-up over a link.
    pub fn start(mut link: Link, port: String, size: Option<usize>, timing_path: Option<PathBuf>, sim: Option<SimRobot>) -> Result<MultiCuber, RobotError> {
        // A real port restarts the Mega: its bootloader listens for a moment first.
        link.wait_banner(Duration::from_millis(if sim.is_some() { 300 } else { 3000 }));
        let hello = link.handshake(5, PROTOCOL)?;
        let fw_size: usize = hello.get("size").and_then(|s| s.parse().ok()).unwrap_or(3);
        let n = size.unwrap_or(fw_size);
        if !SIZES.contains(&n) {
            return Err(RobotError::new("bad_size", format!("the firmware is set for a {0}x{0}x{0}, which MultiCuber does not take", n)));
        }
        let t = Duration::from_secs(2);
        if n != fw_size {
            link.command(&format!("SIZE {}", n), t)?;
        }
        link.command("SPEED MEDIUM", t)?;
        let before = link.info.len();
        let cal = match link.command("CAL", t) {
            Ok(_) => link.info[before..].iter().find_map(|l| Cal::parse(l)).filter(|c| c.n == n).unwrap_or_else(|| Cal::default_for(n)),
            Err(_) => Cal::default_for(n),
        };
        let mut timing = TimeModel::new(cal.clone());
        if let Some(path) = &timing_path {
            timing.absorb(&timing::load_saved(path)["sizes"][n.to_string()]);
        }
        let mut hello = hello;
        hello.insert("size".into(), n.to_string());
        let mut robot = MultiCuber {
            link,
            port,
            n,
            hello,
            cal,
            timing,
            timing_path,
            state: RobotState::default(),
            state_known: false,
            enabled: false,
            sim,
            b_rule: BRule::Legacy,
        };
        robot.sync()?;
        Ok(robot)
    }

    pub fn state(&self) -> RobotState {
        self.state
    }

    pub fn sim(&self) -> Option<&SimRobot> {
        self.sim.as_ref()
    }

    fn planned(&self, ops: Vec<Op>) -> Plan {
        // ENABLE before, DISABLE after, as robot.py does for every scan and solve.
        let enable = Op { cmd: Cmd::Enable, remap: Remap::None, photo: None, mv: None, lift_before: self.state.lift };
        let end = ops.last().map_or(self.state.lift, |o| match o.cmd {
            Cmd::Lift(l) => l,
            _ => o.lift_before,
        });
        let disable = Op { cmd: Cmd::Disable, remap: Remap::None, photo: None, mv: None, lift_before: end };
        let mut out = Vec::with_capacity(ops.len() + 2);
        let mut eta = 0.0;
        for op in std::iter::once(enable).chain(ops).chain(std::iter::once(disable)) {
            let est = if op.cmd == Cmd::Enable && self.enabled { 0.0 } else { self.timing.op_ms(&op) };
            eta += est;
            let (cmd, arg) = op.parts();
            out.push(PlannedOp {
                cmd: cmd.to_string(),
                arg,
                est_ms: (est * 10.0).round() / 10.0,
                mv: op.mv.map(|m| m as usize),
                photo: op.photo.map(|p| PhotoSpec { index: p.index as usize, face: p.face, rotate: p.rotate }),
            });
        }
        Plan { ops: out, eta_ms: eta.round() }
    }

    /// The planner's commands for a solution (without ENABLE/DISABLE).
    /// A planner from where the robot is. The legacy planner never leaves
    /// the lift at the scanning height, and never goes from there to a
    /// turning level: if a stopped run left it there, it comes down first.
    fn planner(&self) -> Planner<'_> {
        let mut p = Planner::new(self.n, self.b_rule).with_timing(&self.timing).from_state(self.state);
        if self.state.lift == Lift::Scan {
            p.lift_to(Lift::Level(0));
        }
        p
    }

    pub fn solution_ops(&self, moves: &[Turn], mode: PlanMode) -> Vec<Op> {
        let mut p = self.planner();
        match mode {
            PlanMode::Legacy => p.solution(moves),
            PlanMode::Optimized => optimize::solution(&mut p, moves, &self.timing),
        }
        p.ops
    }

    fn abort(&mut self) {
        // Whatever is running finishes; DISABLE comes right after it.
        self.state_known = false;
        let _ = self.link.send("DISABLE");
        self.enabled = false;
        // Read the answers still on their way, so that the next command gets its own.
        let deadline = Instant::now() + Duration::from_millis(3000);
        while let Ok(Some(reply)) = self.link.read_reply(deadline) {
            if reply == Reply::Ok("DISABLE".into()) {
                break;
            }
        }
    }

    fn save_timing(&self) {
        if let Some(path) = &self.timing_path {
            let _ = timing::save(path, &self.timing);
        }
    }
}

/// A plan's command back as an op, given where the lift was before it.
fn op_of(p: &PlannedOp, lift: Lift) -> Op {
    let cmd = match p.cmd.as_str() {
        "LIFT" => Cmd::Lift(Lift::parse(&p.arg).unwrap_or(lift)),
        "FLIP" => Cmd::Flip { up: p.arg.starts_with("UP"), normal: p.arg.ends_with("NORMAL") },
        "ROT" => Cmd::Rot(p.arg.parse().unwrap_or(1)),
        "ENABLE" => Cmd::Enable,
        "DISABLE" => Cmd::Disable,
        _ => Cmd::State,
    };
    Op { cmd, remap: Remap::None, photo: None, mv: None, lift_before: lift }
}

impl Robot for MultiCuber {
    fn driver_id(&self) -> &'static str {
        "multicuber"
    }

    fn port(&self) -> String {
        self.port.clone()
    }

    fn size(&self) -> usize {
        self.n
    }

    fn info(&self) -> Value {
        json!(self.hello)
    }

    fn scan_photos(&self) -> Vec<PhotoSpec> {
        planner::SCAN_PHOTOS.iter().enumerate().map(|(i, &(face, rotate))| PhotoSpec { index: i, face, rotate }).collect()
    }

    fn sync(&mut self) -> Result<(), RobotError> {
        let reply = self.link.command("STATE", Duration::from_secs(3))?;
        let f = link::fields(&reply);
        let height: i32 = f.get("lift").and_then(|v| v.parse().ok()).unwrap_or(-1);
        let lift = self.cal.level_at(height).ok_or_else(|| {
            RobotError::new(
                "position",
                format!("the lift is at height {}, which is no level: bring it down (LIFT 0), or put it down by hand and send ZERO", height),
            )
        })?;
        self.state = RobotState { lift, flip_up: f.get("flip").is_none_or(|v| v == "UP") };
        self.enabled = f.get("enabled").is_some_and(|v| v == "1");
        self.state_known = true;
        Ok(())
    }

    fn plan_scan(&self) -> Plan {
        let mut p = self.planner();
        p.scan(false);
        self.planned(p.ops)
    }

    fn plan_solution(&self, moves: &[Turn], mode: PlanMode) -> Plan {
        self.planned(self.solution_ops(moves, mode))
    }

    fn estimate_ms(&self, moves: &[Turn], mode: PlanMode) -> f64 {
        match mode {
            PlanMode::Legacy => {
                let mut p = Planner::costing(self.n, self.b_rule, &self.timing).from_state(self.state);
                if self.state.lift == Lift::Scan {
                    p.lift_to(Lift::Level(0));
                }
                p.solution(moves);
                p.ms
            }
            PlanMode::Optimized => {
                let mut p = self.planner();
                optimize::solution(&mut p, moves, &self.timing);
                p.ms
            }
        }
    }

    fn execute(&mut self, plan: &Plan, stop: &StopFlag, progress: &mut dyn FnMut(Progress) -> Result<(), RobotError>) -> Result<(), RobotError> {
        if !self.state_known {
            self.sync()?;
        }
        let ops = &plan.ops;
        let n = ops.len();
        let lines: Vec<String> = ops.iter().map(|o| if o.arg.is_empty() { o.cmd.clone() } else { format!("{} {}", o.cmd, o.arg) }).collect();
        // Each command as the time model knows it, with where the lift was before it.
        let mut parsed = Vec::with_capacity(n);
        let (mut lift, mut flip_up) = (self.state.lift, self.state.flip_up);
        for o in ops {
            let op = op_of(o, lift);
            match op.cmd {
                Cmd::Lift(l) => lift = l,
                Cmd::Flip { up, .. } => flip_up = up,
                _ => {}
            }
            parsed.push(op);
        }
        let one_way = Duration::from_secs_f64(self.timing.ping_ms / 2000.0);
        let est = |i: usize| Duration::from_secs_f64(ops[i].est_ms.max(0.0) / 1000.0);
        let mut started: Vec<Option<Instant>> = vec![None; n];
        let (mut sent, mut done) = (0usize, 0usize);
        let mut bytes_in_flight: usize = 0;
        while done < n {
            if stop.is_set() {
                self.abort();
                return Err(RobotError::new("stopped", "stopped"));
            }
            // Send what may go now.
            let mut next_send: Option<Instant> = None;
            while sent < n && sent - done < MAX_IN_FLIGHT {
                if (done..sent).any(|k| ops[k].photo.is_some()) {
                    break; // the robot stands still for the picture
                }
                if sent > done && bytes_in_flight + lines[sent].len() + 1 > MAX_BYTES {
                    break;
                }
                if sent > done {
                    if let Some(start) = started[done] {
                        let due = (start + est(done)).checked_sub(Duration::from_secs_f64(LEAD_MS / 1000.0)).unwrap_or(start);
                        if Instant::now() < due {
                            next_send = Some(due);
                            break;
                        }
                    }
                }
                if let Err(e) = self.link.send(&lines[sent]) {
                    self.state_known = false;
                    return Err(e);
                }
                bytes_in_flight += lines[sent].len() + 1;
                if sent == done {
                    started[done] = Some(Instant::now() + one_way);
                    progress(Progress::Started { i: done })?;
                }
                sent += 1;
            }
            let now = Instant::now();
            let wait = next_send.map_or(Duration::from_millis(5), |t| t.saturating_duration_since(now).min(Duration::from_millis(5)));
            match self.link.read_reply(now + wait) {
                Err(e) => {
                    self.state_known = false;
                    return Err(e);
                }
                Ok(None) => {
                    let start = started[done].unwrap_or(now);
                    if Instant::now() > start + 2 * est(done) + Duration::from_secs(3) {
                        self.abort();
                        return Err(RobotError::new("timeout", format!("no answer to {} from the robot", lines[done])));
                    }
                }
                Ok(Some(Reply::Err(text))) => {
                    self.abort();
                    return Err(RobotError::new("robot_refused", format!("{} refused: {}", lines[done], text)));
                }
                Ok(Some(Reply::Ok(_))) => {
                    let finished = Instant::now().checked_sub(one_way).unwrap_or_else(Instant::now);
                    let start = started[done].unwrap_or(finished);
                    let actual = finished.saturating_duration_since(start).as_secs_f64() * 1000.0;
                    self.timing.record(&parsed[done], actual);
                    bytes_in_flight = bytes_in_flight.saturating_sub(lines[done].len() + 1);
                    match parsed[done].cmd {
                        Cmd::Enable => self.enabled = true,
                        Cmd::Disable => self.enabled = false,
                        _ => {}
                    }
                    let result = progress(Progress::Done { i: done, actual_ms: (actual * 10.0).round() / 10.0 }).and_then(|_| match ops[done].photo {
                        Some(photo) => progress(Progress::Photo { i: done, photo }),
                        None => Ok(()),
                    });
                    if let Err(e) = result {
                        self.abort();
                        return Err(e);
                    }
                    done += 1;
                    if done < sent {
                        // It was waiting in the robot's buffer and started as this one ended.
                        started[done] = Some(finished);
                        progress(Progress::Started { i: done })?;
                    }
                }
            }
        }
        self.state = RobotState { lift, flip_up };
        self.state_known = true;
        self.save_timing();
        Ok(())
    }

    fn emergency_stop(&mut self) {
        self.abort();
    }

    fn park(&mut self) -> Result<(), RobotError> {
        if !self.state_known {
            self.sync()?;
        }
        let mut p = Planner::new(self.n, self.b_rule).from_state(self.state);
        p.lift_to(Lift::Level(0));
        p.park();
        if p.ops.is_empty() {
            return Ok(());
        }
        let plan = self.planned(p.ops);
        self.execute(&plan, &StopFlag::default(), &mut |_| Ok(()))
    }

    fn after_run_note(&mut self) -> Option<String> {
        let sim = self.sim.as_ref()?;
        let v = sim.violations();
        Some(format!(
            "simulated robot: cube {}{}{}",
            if sim.is_solved() { "solved" } else { "not solved" },
            if sim.overflows() > 0 { format!(", {} bytes lost to a full receive buffer", sim.overflows()) } else { String::new() },
            if v.is_empty() { String::new() } else { format!(", unsafe steps: {}", v.join("; ")) }
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn close(&mut self) {
        self.save_timing();
        if let Some(s) = &self.sim {
            s.close();
        }
    }
}
