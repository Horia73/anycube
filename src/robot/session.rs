//! Scans and solves on the connected robot, as the local server's robot API
//! offers them (robots-spec/API.md): commands come in, events go out in a
//! numbered list that the page reads with long polls.
//!
//! A run starts with a scan (or with a solve, if there was none) and ends
//! with `done` or `error`. The robot is driven from a thread of its own;
//! a stop reaches it through its `StopFlag` within a few milliseconds.
//!
//! Robots are found by `detect` as they are plugged in (`list`). A robot
//! that is pulled out is noticed within half a second, running or not: the
//! run ends with the error `disconnected`, the connection is dropped and a
//! `disconnected` event tells the page, which goes back to watching.

use super::detect::{Detector, Known};
use super::fastest::{self, Options};
use super::{drivers, ConnectOptions, Progress, Robot, RobotError, StopFlag};
use crate::api::{check, Input};
use serde_json::{json, Map, Value};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long the page has to grab a frame after a `photo` event.
pub const PHOTO_ACK: Duration = Duration::from_secs(3);
/// Events kept for late readers.
const KEEP_EVENTS: usize = 5000;
/// How often a connected robot's port is looked for.
const WATCH: Duration = Duration::from_millis(500);
/// The longest a read of the events is held open.
pub const MAX_WAIT: Duration = Duration::from_secs(10);

/// What a session is set up with.
#[derive(Clone, Debug)]
pub struct Config {
    pub settings_dir: Option<PathBuf>,
    /// Offer the simulated robot (port `sim`), its motions taking this fraction of the real time.
    pub sim_scale: Option<f64>,
    pub max_size: usize,
    /// Wait this long after the motion before a picture (the camera's lag, the cube's shaking).
    pub photo_settle: Duration,
    pub search: Options,
}

impl Config {
    /// From the environment: `ANYCUBE_ROBOT_SIM` (time scale),
    /// `ANYCUBE_ROBOT_PHOTO_SETTLE_MS`, `ANYCUBE_ROBOT_SEARCH_MS`.
    pub fn from_env(settings_dir: Option<PathBuf>, max_size: usize) -> Config {
        let var = |k: &str| std::env::var(k).ok().and_then(|v| v.trim().parse::<f64>().ok());
        let mut search = Options::default();
        if let Some(ms) = var("ANYCUBE_ROBOT_SEARCH_MS") {
            search.search_ms = ms.max(0.0);
        }
        Config {
            settings_dir,
            sim_scale: var("ANYCUBE_ROBOT_SIM").filter(|s| *s >= 0.0),
            max_size,
            photo_settle: Duration::from_secs_f64(var("ANYCUBE_ROBOT_PHOTO_SETTLE_MS").unwrap_or(100.0).max(0.0) / 1000.0),
            search,
        }
    }
}

struct Run {
    id: String,
    started: Instant,
    scan_ms: Option<f64>,
}

#[derive(Default)]
struct State {
    connected: Option<Value>,
    events: VecDeque<Value>,
    seq: u64,
    run: Option<Run>,
    /// A thread is driving the robot.
    busy: bool,
    acks: HashSet<(String, usize)>,
    count: u64,
    /// Counts the connections: a watcher of an earlier one stops.
    generation: u64,
    /// The robot was found to be gone (why); the connection is about to be dropped.
    lost: Option<String>,
    /// What a simulated robot's camera sees, when asked.
    camera: Option<Arc<dyn Fn() -> String + Send + Sync>>,
}

struct Inner {
    state: Mutex<State>,
    changed: Condvar,
    robot: Mutex<Option<Box<dyn Robot>>>,
    stop: StopFlag,
    config: Config,
    detector: Detector,
}

#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

fn err(code: &str, message: impl Into<String>) -> Value {
    json!({"ok": false, "error": {"code": code, "message": message.into()}})
}

fn from_robot(e: RobotError) -> Value {
    err(e.code, e.message)
}

impl Session {
    pub fn new(config: Config) -> Session {
        Session::with_detector(config, Detector::system())
    }

    /// With the ports found some other way (the tests' own).
    pub fn with_detector(config: Config, detector: Detector) -> Session {
        Session {
            inner: Arc::new(Inner {
                state: Mutex::default(),
                changed: Condvar::new(),
                robot: Mutex::new(None),
                stop: StopFlag::default(),
                config,
                detector,
            }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Adds an event; `run` stamps it with the run and the time since the run started.
    fn emit(&self, kind: &str, run: Option<&str>, fields: Value) {
        let mut s = self.inner.state.lock().unwrap();
        self.emit_locked(&mut s, kind, run, fields);
    }

    fn emit_locked(&self, s: &mut State, kind: &str, run: Option<&str>, fields: Value) {
        s.seq += 1;
        let mut e = Map::new();
        e.insert("seq".into(), json!(s.seq));
        e.insert("type".into(), json!(kind));
        let t_ms = match (&s.run, run) {
            (Some(r), Some(id)) if r.id == id => r.started.elapsed().as_secs_f64() * 1000.0,
            _ => 0.0,
        };
        if let Some(id) = run {
            e.insert("run".into(), json!(id));
        }
        e.insert("t_ms".into(), json!(t_ms.round()));
        if let Value::Object(f) = fields {
            e.extend(f);
        }
        s.events.push_back(Value::Object(e));
        while s.events.len() > KEEP_EVENTS {
            s.events.pop_front();
        }
        self.inner.changed.notify_all();
    }

    fn log(&self, run: Option<&str>, text: impl Into<String>) {
        self.emit("log", run, json!({"text": text.into()}));
    }

    /// Events after `since`, waiting up to `wait` for one.
    pub fn events(&self, since: u64, wait: Duration) -> Value {
        let deadline = Instant::now() + wait.min(MAX_WAIT);
        let mut s = self.inner.state.lock().unwrap();
        while s.seq <= since {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            s = self.inner.changed.wait_timeout(s, left).unwrap().0;
        }
        let events: Vec<Value> = s.events.iter().filter(|e| e["seq"].as_u64().is_some_and(|q| q > since)).cloned().collect();
        json!({"seq": s.seq, "events": events})
    }

    /// `GET /api/robots`: the drivers with the ports that may be theirs, what
    /// each port carries as far as it is known, the connected robot. At once,
    /// from what is known; with `probe`, ports not asked yet are asked in the
    /// background (each once: opening a port restarts an Arduino).
    pub fn list(&self, probe: bool) -> Value {
        let sim = self.inner.config.sim_scale.is_some();
        let views = self.inner.detector.ports(probe);
        let drivers: Vec<Value> = drivers()
            .iter()
            .map(|d| {
                let mut ports: Vec<&str> = views.iter().filter(|v| v.claims.contains(&d.id())).map(|v| v.id.path.as_str()).collect();
                if sim && d.id() == "multicuber" {
                    ports.insert(0, super::multicuber::SIM_PORT);
                }
                json!({"id": d.id(), "name": d.name(), "sizes": d.sizes(), "ports": ports})
            })
            .collect();
        let mut ports: Vec<Value> = Vec::new();
        if sim {
            ports.push(json!({"path": super::multicuber::SIM_PORT, "driver": "multicuber", "busy": false}));
        }
        for v in &views {
            let mut p = Map::new();
            p.insert("path".into(), json!(v.id.path));
            p.insert("busy".into(), json!(v.known == Known::Busy));
            match &v.known {
                Known::Robot(driver, info) => {
                    p.insert("driver".into(), json!(driver));
                    p.insert("info".into(), info.clone());
                }
                Known::Nothing => {
                    p.insert("driver".into(), Value::Null);
                }
                // Not asked yet, or not to be asked: `driver` is left out.
                Known::Waiting | Known::Probing | Known::Busy => {}
            }
            ports.push(Value::Object(p));
        }
        let s = self.inner.state.lock().unwrap();
        json!({"drivers": drivers, "ports": ports, "connected": s.connected.clone().unwrap_or(Value::Null)})
    }

    pub fn connect(&self, body: &Value) -> Value {
        let Some(driver) = drivers().into_iter().find(|d| Some(d.id()) == body["driver"].as_str()) else {
            return err("bad_driver", "no such robot driver");
        };
        if self.inner.state.lock().unwrap().busy {
            return err("busy", "the robot is running; stop it first");
        }
        self.disconnect();
        let detector = &self.inner.detector;
        let sim = super::multicuber::SIM_PORT;
        let port = match body["port"].as_str().filter(|p| !p.is_empty()) {
            Some(p) => p.to_string(),
            None if self.inner.config.sim_scale.is_some() && driver.id() == "multicuber" => sim.to_string(),
            // The port known to carry this robot, else the first that may.
            None => {
                let views = detector.ports(false);
                let mine = |v: &&super::detect::PortView| v.claims.contains(&driver.id());
                let known = views.iter().find(|v| matches!(v.known, Known::Robot(d, _) if d == driver.id()));
                match known.or_else(|| views.iter().filter(mine).find(|v| matches!(v.known, Known::Waiting | Known::Probing))) {
                    Some(v) => v.id.path.clone(),
                    None => return err("no_port", "no robot port found; is the robot plugged in?"),
                }
            }
        };
        // No probe of this port from now on; one that is under way ends first.
        let view = detector.reserve(Some(&port));
        if let Some(v) = view.filter(|v| v.known == Known::Nothing && v.claims.contains(&driver.id()) && v.at.elapsed() < Duration::from_secs(3)) {
            // Asked a moment ago: opening it again would only restart the board to hear the same.
            detector.reserve(None);
            return err("no_answer", format!("the board on {} does not answer as {}", v.id.path, driver.name()));
        }
        let opts = ConnectOptions { settings_dir: self.inner.config.settings_dir.clone(), sim_scale: self.inner.config.sim_scale };
        let size = body["size"].as_u64().map(|n| n as usize);
        let robot = match driver.connect(&port, size, &opts) {
            Ok(r) => r,
            Err(e) => {
                detector.reserve(None);
                return from_robot(e);
            }
        };
        let (size, info) = (robot.size(), robot.info());
        detector.identified(&port, driver.id(), info.clone());
        let connected = json!({"driver": driver.id(), "port": port, "size": size, "info": info});
        let present = robot.presence();
        let camera = robot.camera();
        *self.inner.robot.lock().unwrap() = Some(robot);
        let generation = {
            let mut s = self.inner.state.lock().unwrap();
            s.camera = camera;
            s.connected = Some(connected);
            s.generation += 1;
            s.lost = None;
            s.generation
        };
        self.log(None, format!("connected to {} on {} ({}x{}x{})", driver.name(), port, size, size, size));
        // Watch for the board being pulled out, running or not.
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(WATCH);
            if me.inner.state.lock().unwrap().generation != generation {
                return;
            }
            if !present() {
                return me.lost(generation, "the robot's board is no longer plugged in");
            }
        });
        // The solver's tables for this size, now, so that a solve never waits for them.
        let me = self.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            crate::api::solve::load(size);
            me.log(None, format!("solver tables for {0}x{0}x{0} ready in {1} ms", size, t.elapsed().as_millis()));
        });
        json!({"ok": true, "size": size, "info": info, "port": port})
    }

    pub fn disconnect(&self) -> Value {
        if self.inner.state.lock().unwrap().busy {
            self.stop();
            // The run thread lets go of the robot within its next few milliseconds.
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.inner.state.lock().unwrap().busy && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let robot = self.inner.robot.lock().unwrap().take();
        if let Some(mut r) = robot {
            if !self.inner.stop.is_set() {
                if let Err(e) = r.park() {
                    self.log(None, format!("could not park the robot: {}", e.message));
                }
            }
            r.close();
            self.log(None, "disconnected");
        }
        self.inner.detector.reserve(None);
        let mut s = self.inner.state.lock().unwrap();
        s.connected = None;
        s.camera = None;
        s.run = None;
        s.lost = None;
        s.generation += 1;
        json!({"ok": true})
    }

    /// The robot is gone (the watcher saw its port leave).
    fn lost(&self, generation: u64, message: &str) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if s.generation != generation || s.connected.is_none() {
                return;
            }
            // No run starts from here on.
            s.lost = Some(message.to_string());
            if s.busy {
                // The run ends at once; its thread drops the connection.
                self.inner.stop.stop();
                return;
            }
        }
        let mut guard = self.inner.robot.lock().unwrap();
        // Unless somebody disconnected (and connected anew) meanwhile.
        if self.inner.state.lock().unwrap().generation != generation {
            return;
        }
        self.drop_connection(&mut guard, None, message);
    }

    /// Lets go of a robot that is gone, without a word to it, and tells the
    /// page: the run, if one was going on, ends with the error
    /// `disconnected`; then `disconnected`, by which time no robot is connected.
    fn drop_connection(&self, robot: &mut Option<Box<dyn Robot>>, run: Option<&str>, message: &str) {
        if let Some(mut r) = robot.take() {
            r.close();
        }
        self.inner.detector.reserve(None);
        let mut s = self.inner.state.lock().unwrap();
        s.connected = None;
        s.camera = None;
        s.lost = None;
        s.generation += 1;
        if let Some(id) = run {
            self.emit_locked(&mut s, "error", Some(id), json!({"code": "disconnected", "message": message}));
        }
        s.run = None;
        self.emit_locked(&mut s, "disconnected", None, json!({"message": message}));
    }

    /// A run failed: the error goes to the page; if the robot is gone, the connection goes too.
    fn failed(&self, run: &str, e: RobotError, robot: &mut Option<Box<dyn Robot>>) {
        let lost = self.inner.state.lock().unwrap().lost.clone();
        let gone = lost.or_else(|| match e.code {
            // The port itself failed.
            "link" => Some(e.message.clone()),
            // Silence: because the board is no longer there?
            "timeout" | "protocol" => robot.as_deref().filter(|r| !(r.presence())()).map(|_| "the robot's board is no longer plugged in".to_string()),
            _ => None,
        });
        match gone {
            Some(message) => self.drop_connection(robot, Some(run), &message),
            None => self.emit("error", Some(run), json!({"code": e.code, "message": e.message})),
        }
    }

    /// Starts a run for a robot that is free: the new run's id.
    fn begin(&self, id: Option<&str>, keep_scan: bool) -> Result<String, Value> {
        let mut s = self.inner.state.lock().unwrap();
        if s.connected.is_none() || s.lost.is_some() {
            return Err(err("not_connected", "no robot is connected"));
        }
        if s.busy {
            return Err(err("busy", "the robot is running"));
        }
        let continuing = keep_scan && id.is_some() && s.run.as_ref().map(|r| r.id.as_str()) == id;
        if !continuing {
            s.count += 1;
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
            let id = id.map(str::to_string).unwrap_or_else(|| format!("r{:x}{:02x}", stamp, s.count % 256));
            s.run = Some(Run { id, started: Instant::now(), scan_ms: None });
        }
        s.busy = true;
        s.acks.clear();
        self.inner.stop.clear();
        Ok(s.run.as_ref().unwrap().id.clone())
    }

    fn end(&self) {
        let mut s = self.inner.state.lock().unwrap();
        s.busy = false;
        self.inner.changed.notify_all();
    }

    pub fn scan(&self) -> Value {
        let id = match self.begin(None, false) {
            Ok(id) => id,
            Err(e) => return e,
        };
        let me = self.clone();
        let run = id.clone();
        std::thread::spawn(move || {
            me.scan_run(&run);
            me.end();
        });
        json!({"ok": true, "run": id})
    }

    fn scan_run(&self, run: &str) {
        let mut guard = self.inner.robot.lock().unwrap();
        let result = match guard.as_mut() {
            None => Err(RobotError::new("not_connected", "no robot is connected")),
            Some(robot) => self.scan_on(run, robot.as_mut()),
        };
        let note = guard.as_mut().and_then(|r| r.after_run_note());
        if let Err(e) = result {
            self.failed(run, e, &mut guard);
        }
        drop(guard);
        if let Some(n) = note {
            self.log(Some(run), n);
        }
    }

    fn scan_on(&self, run: &str, robot: &mut dyn Robot) -> Result<(), RobotError> {
        let t0 = Instant::now();
        robot.sync()?;
        let plan = robot.plan_scan();
        self.emit("scan-start", Some(run), json!({"steps": plan.ops.len(), "eta_ms": plan.eta_ms}));
        let ops = plan.ops.clone();
        robot.execute(&plan, &self.inner.stop, &mut |p| self.progress(run, &ops, p))?;
        let scan_ms = (t0.elapsed().as_secs_f64() * 1000.0).round();
        if let Some(r) = self.inner.state.lock().unwrap().run.as_mut().filter(|r| r.id == run) {
            r.scan_ms = Some(scan_ms);
        }
        self.emit("scan-done", Some(run), json!({"scan_ms": scan_ms}));
        Ok(())
    }

    /// What `execute` reports, as events; pictures wait for the page's ack.
    fn progress(&self, run: &str, ops: &[super::PlannedOp], p: Progress) -> Result<(), RobotError> {
        match p {
            Progress::Started { i } => {
                if let Some(m) = ops[i].mv {
                    self.emit("move", Some(run), json!({"move": m, "est_ms": ops[i].est_ms}));
                }
                Ok(())
            }
            Progress::Done { i, actual_ms } => {
                self.emit("op", Some(run), json!({"i": i, "cmd": ops[i].cmd, "arg": ops[i].arg, "actual_ms": actual_ms}));
                Ok(())
            }
            Progress::Photo { i, photo } => {
                let settle = self.inner.config.photo_settle;
                if !settle.is_zero() {
                    std::thread::sleep(settle);
                }
                // The robot's time to the next picture (to the end of the scan after the last).
                let rest = &ops[(i + 1).min(ops.len())..];
                let next = rest.iter().position(|o| o.photo.is_some());
                let moving: f64 = rest[..next.map_or(rest.len(), |k| k + 1)].iter().map(|o| o.est_ms).sum();
                let next_ms = (moving + if next.is_some() { settle.as_secs_f64() * 1000.0 } else { 0.0 }).round();
                let mut body = json!({"index": photo.index, "face": photo.face.to_string(), "rotate": photo.rotate, "next_ms": next_ms});
                // A simulated robot shows what its camera would: a page is tried against it with no cube.
                let camera = self.inner.state.lock().unwrap().camera.clone();
                if let Some(seen) = camera {
                    body["sim"] = json!(seen());
                }
                self.emit("photo", Some(run), body);
                let deadline = Instant::now() + PHOTO_ACK;
                let mut s = self.inner.state.lock().unwrap();
                loop {
                    if s.acks.contains(&(run.to_string(), photo.index)) {
                        return Ok(());
                    }
                    if self.inner.stop.is_set() {
                        return Err(RobotError::new("stopped", "stopped"));
                    }
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(RobotError::new(
                            "photo_timeout",
                            format!("photo {} was not taken within {} s", photo.index, PHOTO_ACK.as_secs()),
                        ));
                    }
                    s = self.inner.changed.wait_timeout(s, left.min(Duration::from_millis(20))).unwrap().0;
                }
            }
        }
    }

    pub fn photo_taken(&self, body: &Value) -> Value {
        let (Some(run), Some(index)) = (body["run"].as_str(), body["index"].as_u64()) else {
            return err("bad_request", "send run and index");
        };
        let mut s = self.inner.state.lock().unwrap();
        if s.run.as_ref().map(|r| r.id.as_str()) != Some(run) {
            return err("bad_run", "no such run");
        }
        s.acks.insert((run.to_string(), index as usize));
        self.inner.changed.notify_all();
        json!({"ok": true})
    }

    pub fn solve(&self, body: &Value) -> Value {
        let robot_size = match &self.inner.state.lock().unwrap().connected {
            Some(c) => c["size"].as_u64().unwrap_or(0) as usize,
            None => return err("not_connected", "no robot is connected"),
        };
        let Some(facelets) = body["facelets"].as_str() else { return err("bad_request", "send facelets") };
        let size = body["size"].as_u64().map_or(robot_size, |n| n as usize);
        if size != robot_size {
            return err("bad_size", format!("the robot is set for a {0}x{0}x{0}, not a {1}x{1}x{1}", robot_size, size));
        }
        let cube = match check(size, Input::Facelets(facelets), self.inner.config.max_size) {
            Ok(c) => c,
            Err(e) => return err(e.code, e.message),
        };
        let analysis_ms = body["analysis_ms"].as_f64().unwrap_or(0.0);
        let id = match self.begin(body["run"].as_str(), true) {
            Ok(id) => id,
            Err(e) => return e,
        };
        let me = self.clone();
        std::thread::spawn(move || {
            me.solve_run(&id, cube, analysis_ms);
            me.end();
        });
        json!({"ok": true})
    }

    fn solve_run(&self, run: &str, cube: crate::api::Checked, analysis_ms: f64) {
        // A solve never waits for tables while it is timed; the connect started loading them.
        crate::api::solve::load(cube.size);
        let mut guard = self.inner.robot.lock().unwrap();
        let result = match guard.as_mut() {
            None => Err(RobotError::new("not_connected", "no robot is connected")),
            Some(robot) => self.solve_on(run, robot.as_mut(), &cube, analysis_ms),
        };
        let note = guard.as_mut().and_then(|r| r.after_run_note());
        if let Err(e) = result {
            self.failed(run, e, &mut guard);
        }
        drop(guard);
        if let Some(n) = note {
            self.log(Some(run), n);
        }
    }

    fn solve_on(&self, run: &str, robot: &mut dyn Robot, cube: &crate::api::Checked, analysis_ms: f64) -> Result<(), RobotError> {
        robot.sync()?;
        let choice = {
            let cost = robot.estimator();
            fastest::choose(cube, &*cost, &self.inner.config.search).map_err(|e| RobotError::new(e.code, e.message))?
        };
        if self.inner.stop.is_set() {
            return Err(RobotError::new("stopped", "stopped"));
        }
        let plan = robot.plan_solution(&choice.moves);
        let compute_ms = (choice.compute_ms * 10.0).round() / 10.0;
        let ops_json: Vec<Value> =
            plan.ops.iter().map(|o| json!({"cmd": o.cmd, "arg": o.arg, "est_ms": o.est_ms, "move": o.mv})).collect();
        self.emit(
            "plan",
            Some(run),
            json!({
                "moves": choice.names(),
                "ops": ops_json,
                "eta_ms": plan.eta_ms,
                "compute_ms": compute_ms,
                "candidates": choice.candidates,
                "metric": "robot-ms",
                "planner": robot.planner_name(),
            }),
        );
        let t_exec = Instant::now();
        let ops = plan.ops.clone();
        robot.execute(&plan, &self.inner.stop, &mut |p| self.progress(run, &ops, p))?;
        let exec_ms = (t_exec.elapsed().as_secs_f64() * 1000.0).round();
        let (scan_ms, total_ms) = {
            let s = self.inner.state.lock().unwrap();
            let r = s.run.as_ref().filter(|r| r.id == run);
            (r.and_then(|r| r.scan_ms).unwrap_or(0.0), r.map_or(0.0, |r| (r.started.elapsed().as_secs_f64() * 1000.0).round()))
        };
        self.emit(
            "done",
            Some(run),
            json!({"times": {"scan_ms": scan_ms, "analysis_ms": analysis_ms, "compute_ms": compute_ms, "exec_ms": exec_ms, "total_ms": total_ms}}),
        );
        Ok(())
    }

    /// Emergency stop: motors off at once; the run ends with `error` `stopped`.
    pub fn stop(&self) -> Value {
        self.inner.stop.stop();
        let (busy, run) = {
            let s = self.inner.state.lock().unwrap();
            (s.busy, s.run.as_ref().map(|r| r.id.clone()))
        };
        if !busy {
            // Nothing is running: switch the motors off from here.
            if let Ok(mut guard) = self.inner.robot.try_lock() {
                if let Some(r) = guard.as_mut() {
                    r.emergency_stop();
                }
            }
            if let Some(id) = run {
                self.emit("error", Some(&id), json!({"code": "stopped", "message": "stopped"}));
                self.inner.state.lock().unwrap().run = None;
            }
        }
        json!({"ok": true})
    }

    /// Waits until no run is going on (for tests and shutdown).
    pub fn wait_idle(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut s = self.inner.state.lock().unwrap();
        while s.busy {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            s = self.inner.changed.wait_timeout(s, left).unwrap().0;
        }
        true
    }

    /// Runs something on the connected robot (tests look inside the simulator this way).
    pub fn with_robot<R>(&self, f: impl FnOnce(&mut dyn Robot) -> R) -> Option<R> {
        let mut guard = self.inner.robot.lock().unwrap();
        guard.as_mut().map(|r| f(r.as_mut()))
    }
}
