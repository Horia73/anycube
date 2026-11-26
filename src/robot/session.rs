//! Scans and solves on the connected robot, as the local server's robot API
//! offers them (robots-spec/API.md): commands come in, events go out in a
//! numbered list that the page reads with long polls.
//!
//! A run starts with a scan (or with a solve, if there was none) and ends
//! with `done` or `error`. The robot is driven from a thread of its own;
//! a stop reaches it through its `StopFlag` within a few milliseconds.

use super::fastest::{self, Options};
use super::{drivers, ConnectOptions, PlanMode, Progress, Robot, RobotError, StopFlag, Turn};
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
    /// From the environment: `ANYCUBE_ROBOT_SIM` (time scale), `ANYCUBE_ROBOT_PLANNER`
    /// (`legacy` or `optimized`), `ANYCUBE_ROBOT_PHOTO_SETTLE_MS`, `ANYCUBE_ROBOT_SEARCH_MS`.
    pub fn from_env(settings_dir: Option<PathBuf>, max_size: usize) -> Config {
        let var = |k: &str| std::env::var(k).ok().and_then(|v| v.trim().parse::<f64>().ok());
        let mut search = Options::default();
        if std::env::var("ANYCUBE_ROBOT_PLANNER").is_ok_and(|v| v == "optimized") {
            search.plan = PlanMode::Optimized;
        }
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
}

struct Inner {
    state: Mutex<State>,
    changed: Condvar,
    robot: Mutex<Option<Box<dyn Robot>>>,
    stop: StopFlag,
    config: Config,
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
        Session {
            inner: Arc::new(Inner {
                state: Mutex::default(),
                changed: Condvar::new(),
                robot: Mutex::new(None),
                stop: StopFlag::default(),
                config,
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

    /// `GET /api/robots`.
    pub fn list(&self) -> Value {
        let sim = self.inner.config.sim_scale.is_some();
        let drivers: Vec<Value> = drivers()
            .iter()
            .map(|d| {
                let mut ports = d.ports();
                if sim && d.id() == "multicuber" {
                    ports.insert(0, "sim".into());
                }
                json!({"id": d.id(), "name": d.name(), "sizes": d.sizes(), "ports": ports})
            })
            .collect();
        let s = self.inner.state.lock().unwrap();
        json!({"drivers": drivers, "connected": s.connected.clone().unwrap_or(Value::Null)})
    }

    pub fn connect(&self, body: &Value) -> Value {
        let Some(driver) = drivers().into_iter().find(|d| Some(d.id()) == body["driver"].as_str()) else {
            return err("bad_driver", "no such robot driver");
        };
        if self.inner.state.lock().unwrap().busy {
            return err("busy", "the robot is running; stop it first");
        }
        self.disconnect();
        let opts = ConnectOptions { settings_dir: self.inner.config.settings_dir.clone(), sim_scale: self.inner.config.sim_scale };
        let port = body["port"].as_str().filter(|p| !p.is_empty());
        let size = body["size"].as_u64().map(|n| n as usize);
        let robot = match driver.connect(port, size, &opts) {
            Ok(r) => r,
            Err(e) => return from_robot(e),
        };
        let (size, info) = (robot.size(), robot.info());
        let connected = json!({"driver": driver.id(), "port": robot.port(), "size": size, "info": info});
        *self.inner.robot.lock().unwrap() = Some(robot);
        self.inner.state.lock().unwrap().connected = Some(connected);
        self.log(None, format!("connected to {} on {} ({}x{}x{})", driver.name(), body["port"].as_str().unwrap_or("its port"), size, size, size));
        // The solver's tables for this size, now, so that a solve never waits for them.
        let me = self.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            crate::api::solve::load(size);
            me.log(None, format!("solver tables for {0}x{0}x{0} ready in {1} ms", size, t.elapsed().as_millis()));
        });
        json!({"ok": true, "size": size, "info": info})
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
        let mut s = self.inner.state.lock().unwrap();
        s.connected = None;
        s.run = None;
        json!({"ok": true})
    }

    /// Starts a run for a robot that is free: the new run's id.
    fn begin(&self, id: Option<&str>, keep_scan: bool) -> Result<String, Value> {
        let mut s = self.inner.state.lock().unwrap();
        if s.connected.is_none() {
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
        let Some(robot) = guard.as_mut() else { return self.emit("error", Some(run), json!({"code": "not_connected", "message": "no robot is connected"})) };
        let t0 = Instant::now();
        if let Err(e) = robot.sync() {
            return self.emit("error", Some(run), json!({"code": e.code, "message": e.message}));
        }
        let plan = robot.plan_scan();
        self.emit("scan-start", Some(run), json!({"steps": plan.ops.len(), "eta_ms": plan.eta_ms}));
        let ops = plan.ops.clone();
        let result = robot.execute(&plan, &self.inner.stop, &mut |p| self.progress(run, &ops, p));
        let note = robot.after_run_note();
        drop(guard);
        match result {
            Ok(()) => {
                let scan_ms = (t0.elapsed().as_secs_f64() * 1000.0).round();
                if let Some(r) = self.inner.state.lock().unwrap().run.as_mut().filter(|r| r.id == run) {
                    r.scan_ms = Some(scan_ms);
                }
                self.emit("scan-done", Some(run), json!({"scan_ms": scan_ms}));
            }
            Err(e) => self.emit("error", Some(run), json!({"code": e.code, "message": e.message})),
        }
        if let Some(n) = note {
            self.log(Some(run), n);
        }
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
            Progress::Photo { photo, .. } => {
                let settle = self.inner.config.photo_settle;
                if !settle.is_zero() {
                    std::thread::sleep(settle);
                }
                self.emit("photo", Some(run), json!({"index": photo.index, "face": photo.face.to_string(), "rotate": photo.rotate}));
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
        let fail = |e: RobotError| self.emit("error", Some(run), json!({"code": e.code, "message": e.message}));
        // A solve never waits for tables while it is timed; the connect started loading them.
        crate::api::solve::load(cube.size);
        let mut guard = self.inner.robot.lock().unwrap();
        let Some(robot) = guard.as_mut() else { return fail(RobotError::new("not_connected", "no robot is connected")) };
        if let Err(e) = robot.sync() {
            return fail(e);
        }
        let opts = self.inner.config.search.clone();
        let robot_ref: &dyn Robot = robot.as_ref();
        let cost = |m: &[Turn], mode: PlanMode| robot_ref.estimate_ms(m, mode);
        let choice = match fastest::choose(&cube, &cost, &opts) {
            Ok(c) => c,
            Err(e) => return fail(RobotError::new(e.code, e.message)),
        };
        if self.inner.stop.is_set() {
            return fail(RobotError::new("stopped", "stopped"));
        }
        let plan = robot.plan_solution(&choice.moves, opts.plan);
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
                "planner": if opts.plan == PlanMode::Optimized { "optimized" } else { "legacy" },
            }),
        );
        let t_exec = Instant::now();
        let ops = plan.ops.clone();
        let result = robot.execute(&plan, &self.inner.stop, &mut |p| self.progress(run, &ops, p));
        let note = robot.after_run_note();
        drop(guard);
        match result {
            Ok(()) => {
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
            }
            Err(e) => fail(e),
        }
        if let Some(n) = note {
            self.log(Some(run), n);
        }
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
