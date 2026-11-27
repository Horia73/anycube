//! MultiCuber (Arduino Mega + RAMPS, LIFT / FLIP / ROT steppers, firmware 2
//! line protocol at 115200 baud): the driver.
//!
//! Connecting opens the port (which restarts the Mega), waits for its
//! banner and says HELLO (protocol 1). HELLO reports the current configuration.
//! Each scan confirms its requested size with SIZE, CAL and STATE. The speed preset is set to
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
//!
//! Scans and solutions are planned in `optimize`: the quickest commands
//! under the time model, each one a command that the 2018 programs send in
//! the same state (`planner::allowed`). A board is identified by the same
//! handshake without anything else being sent (`probe`); the port it leaves
//! open is taken over by the connect that follows.

pub mod legacy;
pub mod link;
pub mod optimize;
pub mod overlap;
pub mod planner;
pub mod sim;
pub mod timing;

use self::link::{Link, Reply, SerialTransport};
use self::optimize::Space;
use self::planner::{Cmd, Lift, Op, Planner, Remap, RobotState};
use self::sim::SimRobot;
use self::timing::{Cal, TimeModel};
use super::{ConnectOptions, Driver, PhotoSpec, Plan, PlannedOp, PortId, Probe, Progress, Robot, RobotError, StopFlag, Turn};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
/// The port of the simulated robot.
pub const SIM_PORT: &str = "sim";
/// A Mega restarts when its port is opened; its bootloader listens this long at most before the firmware speaks.
const BOOT_WAIT: Duration = Duration::from_millis(3000);
/// How long the port of a board just identified is kept open for the connect that follows.
const HOLD_PROBED: Duration = Duration::from_secs(8);

/// The link of the board a probe has just identified. Opening the port
/// again would restart the Mega and cost its three seconds of start-up a
/// second time, so the connect that follows takes this one over.
struct Probed {
    port: String,
    device: Box<dyn serialport::SerialPort>,
    at: Instant,
}

static PROBED: Mutex<Option<Probed>> = Mutex::new(None);

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

    /// The Mega's own USB port, or a CH340 / FTDI clone's.
    fn claims(&self, port: &PortId) -> bool {
        port.path == SIM_PORT
            || ["/dev/cu.usbmodem", "/dev/cu.usbserial", "/dev/cu.wchusbserial", "/dev/ttyACM", "/dev/ttyUSB", "COM"].iter().any(|prefix| port.path.starts_with(prefix))
    }

    /// Opens the port (the Mega restarts), waits for the firmware's banner
    /// and says HELLO: nothing else is sent, nothing moves.
    fn probe(&self, port: &str) -> Probe {
        if port == SIM_PORT {
            return Probe::Robot(json!({"fw": "2.2.0-sim", "proto": PROTOCOL.to_string()}));
        }
        let Ok(transport) = SerialTransport::open(port, BAUD) else { return Probe::Busy };
        let device = transport.into_port();
        let Ok(reader) = device.try_clone() else { return Probe::Busy };
        let mut link = Link::new(Box::new(SerialTransport::from_port(reader)));
        link.wait_banner(BOOT_WAIT);
        match link.handshake(2, PROTOCOL) {
            Ok(hello) => {
                let at = Instant::now();
                *PROBED.lock().unwrap() = Some(Probed { port: port.to_string(), device, at });
                // Nobody connected: let the port go.
                std::thread::spawn(move || {
                    std::thread::sleep(HOLD_PROBED);
                    let mut held = PROBED.lock().unwrap();
                    if held.as_ref().is_some_and(|p| p.at == at) {
                        *held = None;
                    }
                });
                Probe::Robot(json!(hello))
            }
            Err(_) => Probe::NotThis,
        }
    }

    fn connect(&self, port: &str, size: Option<usize>, opts: &ConnectOptions) -> Result<Box<dyn Robot>, RobotError> {
        if let Some(n) = size.filter(|n| !SIZES.contains(n)) {
            return Err(RobotError::new("bad_size", format!("MultiCuber takes the sizes {:?}, not {}", SIZES, n)));
        }
        let timing_path = opts.settings_dir.as_ref().map(|d| d.join("robot-multicuber-timing.json"));
        let boxed = |r: MultiCuber| Box::new(r) as Box<dyn Robot>;
        if port == SIM_PORT {
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
            let robot = SimRobot::new(size.unwrap_or(3), seed, opts.sim_scale.unwrap_or(1.0));
            let link = Link::new(robot.transport());
            return MultiCuber::start(link, port.to_string(), size, timing_path, Some(robot), Duration::from_millis(300)).map(boxed);
        }
        // The board a probe has just identified is still open and awake.
        let probed = {
            let mut held = PROBED.lock().unwrap();
            if held.as_ref().is_some_and(|p| p.port == port) { held.take() } else { None }
        };
        if let Some(p) = probed.filter(|p| p.at.elapsed() < HOLD_PROBED) {
            return connect_serial(p.device, port, size, opts, true);
        }
        connect_serial(SerialTransport::open(port, BAUD)?.into_port(), port, size, opts, false)
    }
}

pub(super) fn connect_serial(device: Box<dyn serialport::SerialPort>, port: &str, size: Option<usize>, opts: &ConnectOptions, awake: bool) -> Result<Box<dyn Robot>, RobotError> {
    if size.is_some_and(|n| !SIZES.contains(&n)) {
        return Err(RobotError::new("bad_size", "MultiCuber requires size 3, 4 or 5"));
    }
    let reader = device.try_clone().map_err(|e| RobotError::new("port", e.to_string()))?;
    let mut legacy = Link::new(Box::new(SerialTransport::from_port(reader)));
    if !awake { legacy.wait_banner(BOOT_WAIT); }
    let hello = legacy.handshake(5, PROTOCOL)?;
    if hello.get("fw").is_some_and(|version| version.starts_with("2.3."))
        && hello.get("backend").is_some_and(|name| name != "STEPPERDRIVER")
    {
        if let Some(n) = size {
            if hello.get("size").and_then(|s| s.parse().ok()) != Some(n) {
                legacy.command(&format!("SIZE {n}"), Duration::from_secs(3))?;
            }
        }
        drop(legacy);
        return super::solution_link::connect_mega(device, port, size);
    }
    let timing = opts.settings_dir.as_ref().map(|d| d.join("robot-multicuber-timing.json"));
    Ok(Box::new(MultiCuber::start(legacy, port.into(), size, timing, None, Duration::ZERO)?))
}

/// Whether a serial port is (still) there.
fn port_present(path: &str) -> bool {
    if path.starts_with('/') {
        std::path::Path::new(path).exists()
    } else {
        super::detect::serial_ports().iter().any(|p| p.path == path)
    }
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
    /// The quickest ways between the robot's states under `timing`.
    space: Arc<Space>,
    /// A command's time was learned since `space` was made.
    space_stale: bool,
    /// Pictures only right after the flipper tipped the cube in front of
    /// the camera, as in 2018 (`ANYCUBE_ROBOT_SCAN_SEATED=0` lifts this).
    pub scan_seated: bool,
}

impl MultiCuber {
    /// The handshake and set-up over a link.
    /// `boot_wait`: how long to wait for the banner of a board that has just restarted.
    pub fn start(mut link: Link, port: String, size: Option<usize>, timing_path: Option<PathBuf>, sim: Option<SimRobot>, boot_wait: Duration) -> Result<MultiCuber, RobotError> {
        if !boot_wait.is_zero() {
            link.wait_banner(boot_wait);
        }
        let hello = link.handshake(5, PROTOCOL)?;
        let fw_size: usize = hello.get("size").and_then(|s| s.parse().ok()).ok_or_else(|| RobotError::new("protocol", "missing firmware size"))?;
        let n = size.unwrap_or(fw_size);
        if !SIZES.contains(&n) {
            return Err(RobotError::new("bad_size", format!("the firmware is set for a {0}x{0}x{0}, which MultiCuber does not take", n)));
        }
        let t = Duration::from_secs(2);
        if n != fw_size {
            link.command(&format!("SIZE {}", n), t)?;
        }
        link.command("SPEED MEDIUM", t)?;
        link.info.clear();
        let cal = match link.command("CAL", t) {
            Ok(_) => link.info.iter().find_map(|l| Cal::parse(l)).filter(|c| c.n == n).unwrap_or_else(|| Cal::default_for(n)),
            Err(_) => Cal::default_for(n),
        };
        let mut timing = TimeModel::new(cal.clone());
        if let Some(path) = &timing_path {
            timing.absorb(&timing::load_saved(path)["sizes"][n.to_string()]);
        }
        let mut hello = hello;
        hello.insert("size".into(), n.to_string());
        let space = Arc::new(Space::new(n, &timing));
        let scan_seated = std::env::var("ANYCUBE_ROBOT_SCAN_SEATED").map_or(true, |v| v.trim() != "0");
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
            space,
            space_stale: false,
            scan_seated,
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
        let enable = Op { cmd: Cmd::Enable, remap: Remap::None, photo: None, mv: None, lift_before: self.state.lift, joined: false };
        let end = ops.last().map_or(self.state.lift, |o| match o.cmd {
            Cmd::Lift(l) => l,
            _ => o.lift_before,
        });
        let disable = Op { cmd: Cmd::Disable, remap: Remap::None, photo: None, mv: None, lift_before: end, joined: false };
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

    /// A command builder from where the robot is.
    fn builder(&self) -> Planner<'_> {
        Planner::new(self.n).with_timing(&self.timing).from_state(self.state)
    }

    // Only acknowledged scan operations are replayed. Scan rotations always
    // move the whole cube. The planner tracks its orientation as well as the axes.
    fn scan_return(&self, completed: &[PlannedOp]) -> (RobotState, Plan) {
        let mut p = self.builder();
        for op in completed {
            match op_of(op, p.lift).cmd {
                Cmd::Lift(to) => p.lift_to(to),
                Cmd::Flip { up, normal } => p.flip(up, normal),
                Cmd::Rot(turns) => p.rot(turns, true, None),
                _ => {}
            }
        }
        let hold = p.state();
        p.ops.clear();
        self.space.return_to_start(&mut p, self.state);
        (hold, self.planned(p.ops))
    }

    /// The planner's commands for a solution (without ENABLE/DISABLE).
    pub fn solution_ops(&self, moves: &[Turn]) -> Vec<Op> {
        let mut p = self.builder();
        self.space.solution(&mut p, moves);
        p.ops
    }

    /// The planner's commands for a scan (without ENABLE/DISABLE), the pictures named.
    pub fn scan_ops(&self) -> Vec<Op> {
        let mut p = self.builder();
        self.space.scan(&mut p, self.scan_seated);
        let mut ops = p.ops;
        optimize::name_photos(self.n, self.state, &mut ops);
        ops
    }

    /// The quickest ways again, after command times were learned or set.
    pub fn refresh(&mut self) {
        self.space = Arc::new(Space::new(self.n, &self.timing));
        self.space_stale = false;
    }

    fn abort(&mut self) {
        // Whatever is running finishes; DISABLE comes right after it.
        self.state_known = false;
        self.enabled = false;
        if self.link.send("DISABLE").is_err() {
            return; // the port is gone: there is nothing to wait for
        }
        // Read the answers still on their way, so that the next command gets its own.
        let deadline = Instant::now() + Duration::from_millis(3000);
        while let Ok(Some(reply)) = self.link.read_reply(deadline) {
            if reply == Reply::Ok("DISABLE".into()) {
                break;
            }
        }
    }

    fn select_calibration(&mut self, n: usize) -> Result<Cal, RobotError> {
        let timeout = Duration::from_secs(2);
        let cmd = format!("SIZE {n}");
        if self.link.command(&cmd, timeout)? != cmd {
            return Err(RobotError::new("protocol", "firmware did not confirm the requested size"));
        }
        self.link.info.clear();
        let reply = self.link.command("CAL", timeout)?;
        if reply.split_whitespace().take(2).collect::<Vec<_>>() != ["CAL", &n.to_string()] {
            return Err(RobotError::new("protocol", "firmware did not confirm the calibration size"));
        }
        let cal = self.link.info.iter().find_map(|line| Cal::parse(line)).filter(|cal| cal.n == n)
            .ok_or_else(|| RobotError::new("protocol", "missing calibration for requested size"))?;
        Ok(cal)
    }

    fn expect(&mut self, command: &str, expected: &str) -> Result<(), RobotError> {
        if self.link.command(command, Duration::from_secs(3))? != expected {
            return Err(RobotError::new("protocol", format!("firmware did not confirm {command}")));
        }
        Ok(())
    }

    // Firmware 2.2 cmdFlip uses only p_.flip and lift_ == 0. SIZE loads
    // a table without moving an axis. Verify EEPROM overrides, then put
    // back the configured table before enabling any motor.
    fn verify_common_flip(&mut self) -> Result<(), RobotError> {
        let n = self.n;
        let check = SIZES.iter().copied().filter(|&size| size != n)
            .map(|size| self.select_calibration(size).map(|cal| cal.flip))
            .collect::<Result<Vec<_>, _>>();
        let active = self.select_calibration(n)?;
        if active.flip <= 0 || check?.iter().any(|&steps| steps != active.flip) {
            return Err(RobotError::new("calibration", "The flipper travel differs between cube-size calibrations."));
        }
        Ok(())
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
    Op { cmd, remap: Remap::None, photo: None, mv: None, lift_before: lift, joined: false }
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

    fn measure_size(&mut self, stop: &StopFlag, read: &mut dyn FnMut() -> Result<usize, RobotError>) -> Result<usize, RobotError> {
        let running = || if stop.is_set() { Err(RobotError::new("stopped", "stopped")) } else { Ok(()) };
        self.sync()?;
        if self.state.lift != Lift::Level(0) {
            return Err(RobotError::new("position", "Lower the lift to its rest position before scanning."));
        }
        running()?;
        if !self.state.flip_up {
            let measurement = read();
            if stop.is_set() { self.abort(); return Err(RobotError::new("stopped", "stopped")); }
            return measurement;
        }
        self.verify_common_flip()?;
        // A missing or unexpected acknowledgement leaves the pose unknown:
        // disable, never attempt a blind inverse. Stop also forbids the undo.
        let result = (|| {
            running()?;
            self.expect("ENABLE", "ENABLE")?;
            running()?;
            self.expect("FLIP DOWN NORMAL", "FLIP DOWN CARRY")?;
            let measurement = read();
            running()?;
            // ENABLE is idempotent, including after the firmware idle timer.
            self.expect("ENABLE", "ENABLE")?;
            running()?;
            self.expect("FLIP UP NORMAL", "FLIP UP CARRY")?;
            self.expect("DISABLE", "DISABLE")?;
            self.sync()?;
            if self.state.lift != Lift::Level(0) || !self.state.flip_up || self.enabled {
                return Err(RobotError::new("position", "Firmware did not confirm the restored size-photo pose."));
            }
            Ok(measurement)
        })();
        match result {
            Ok(measurement) => measurement,
            Err(error) => { self.abort(); Err(error) }
        }
    }

    fn prepare_size(&mut self, n: usize) -> Result<(), RobotError> {
        if !SIZES.contains(&n) {
            return Err(RobotError::new("bad_size", "MultiCuber requires size 3, 4 or 5"));
        }
        let cal = self.select_calibration(n)?;
        if let Some(sim) = &self.sim {
            sim.fit_cube(n);
        }
        if cal == self.cal { return Ok(()); }
        let mut timing = TimeModel::new(cal.clone());
        if let Some(path) = &self.timing_path {
            timing.absorb(&timing::load_saved(path)["sizes"][n.to_string()]);
        }
        self.n = n;
        self.hello.insert("size".into(), n.to_string());
        self.cal = cal;
        self.timing = timing;
        self.space_stale = true;
        Ok(())
    }

    fn info(&self) -> Value {
        let mut info = json!(self.hello);
        info["size_photo"] = json!(true);
        info["restore"] = json!(true);
        info["restore_after_scan"] = json!(true);
        info
    }

    fn sync(&mut self) -> Result<(), RobotError> {
        let reply = self.link.command("STATE", Duration::from_secs(3))?;
        let f = link::fields(&reply);
        if f.get("size").and_then(|v| v.parse().ok()) != Some(self.n) {
            return Err(RobotError::new("bad_size", "firmware state reports a different size"));
        }
        let height: i32 = f.get("lift").and_then(|v| v.parse().ok()).unwrap_or(-1);
        let lift = self.cal.level_at(height).ok_or_else(|| {
            RobotError::new(
                "position",
                format!("the lift is at height {}, which is no level: bring it down (LIFT 0), or put it down by hand and send ZERO", height),
            )
        })?;
        self.state = RobotState { lift, flip_up: match f.get("flip").map(String::as_str) {
            Some("UP") => true,
            Some("DOWN") => false,
            _ => return Err(RobotError::new("position", "firmware did not report the flipper position")),
        } };
        self.enabled = f.get("enabled").is_some_and(|v| v == "1");
        self.state_known = true;
        if self.space_stale {
            self.refresh();
        }
        Ok(())
    }

    fn plan_scan(&self) -> Plan {
        self.planned(self.scan_ops())
    }

    fn scan_return_plan(&self, plan: &Plan) -> Option<Plan> {
        Some(self.scan_return(&plan.ops).1)
    }

    fn plan_solution(&self, moves: &[Turn]) -> Plan {
        self.planned(self.solution_ops(moves))
    }

    fn estimator(&self) -> Box<dyn Fn(&[Turn]) -> f64 + Send> {
        let (space, start) = (self.space.clone(), self.state);
        Box::new(move |moves| space.cost(start, moves))
    }

    fn presence(&self) -> Arc<dyn Fn() -> bool + Send + Sync> {
        match &self.sim {
            Some(sim) => {
                let sim = sim.clone();
                Arc::new(move || !sim.is_closed())
            }
            None => {
                let port = self.port.clone();
                Arc::new(move || port_present(&port))
            }
        }
    }

    fn camera(&self) -> Option<Arc<dyn Fn() -> String + Send + Sync>> {
        let sim = self.sim.clone()?;
        Some(Arc::new(move || sim.seen()))
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
                    if self.timing.record(&parsed[done], actual) {
                        self.space_stale = true;
                    }
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
                        if e.code == "no_frame" && ops[done].photo.is_some() && !stop.is_set() {
                            // No later command can be in flight across a photo barrier.
                            let (hold, recovery) = self.scan_return(&ops[..=done]);
                            self.state = hold;
                            self.execute(&recovery, stop, &mut |_| Ok(()))?;
                            return Err(RobotError::new("scan_restored", "The camera gave no picture. The robot returned the cube to its starting pose."));
                        }
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
        let mut p = self.builder();
        self.space.park(&mut p);
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
