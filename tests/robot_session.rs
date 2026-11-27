//! A whole run on the simulated MultiCuber through the session the server
//! uses: connect, scan (the pictures acknowledged as the page does), solve
//! from the facelets of the cube the scan left, and the events as
//! robots-spec/API.md lists them. The simulated cube ends solved, no byte
//! is lost to the robot's receive buffer, and nothing unsafe is done.
//! Boards are listed as they are plugged in, each asked once what it is;
//! a robot pulled out, running or not, ends its run and its connection.
#![cfg(feature = "robots")]

use anycube::robot::detect::Detector;
use anycube::robot::multicuber::sim::SimRobot;
use anycube::robot::multicuber::MultiCuber;
use anycube::robot::session::{Config, Session};
use anycube::robot::{PortId, Probe};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn tables() {
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
}

fn session(scale: f64, settle_ms: u64) -> Session {
    tables();
    let dir = std::env::temp_dir().join(format!("anycube-robot-session-{}-{}", std::process::id(), scale));
    Session::new(Config {
        settings_dir: Some(dir),
        sim_scale: Some(scale),
        max_size: 50,
        photo_settle: Duration::from_millis(settle_ms),
        search: Default::default(),
    })
}

fn sim_facelets(s: &Session) -> (String, bool, usize, Vec<String>) {
    s.with_robot(|r| {
        let mc = r.as_any().downcast_ref::<MultiCuber>().unwrap();
        let sim = mc.sim().unwrap();
        (sim.facelets(), sim.is_solved(), sim.overflows(), sim.violations())
    })
    .unwrap()
}

fn sim_log(s: &Session) -> Vec<String> {
    s.with_robot(|r| r.as_any().downcast_ref::<MultiCuber>().unwrap().sim().unwrap().log()).unwrap()
}

/// Reads events until one of `until` arrives (acknowledging pictures if asked); all of them.
fn read_until(s: &Session, since: &mut u64, until: &[&str], ack: bool, limit: Duration) -> Vec<Value> {
    let mut out = Vec::new();
    let deadline = Instant::now() + limit;
    loop {
        assert!(Instant::now() < deadline, "no {:?} in time; events: {:?}", until, out);
        let got = s.events(*since, Duration::from_secs(1));
        for e in got["events"].as_array().unwrap() {
            *since = e["seq"].as_u64().unwrap();
            out.push(e.clone());
            if ack && e["type"] == "photo" {
                let r = s.photo_taken(&json!({"run": e["run"], "index": e["index"]}));
                assert_eq!(r["ok"], true, "{}", r);
            }
            if until.contains(&e["type"].as_str().unwrap()) {
                return out;
            }
        }
    }
}

fn of_type<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == kind).collect()
}

#[test]
fn scan_and_solve_on_the_simulated_robot() {
    let s = session(0.0, 0);
    let list = s.list(true);
    assert_eq!(list["drivers"][0]["id"], "multicuber");
    assert_eq!(list["drivers"][0]["ports"][0], "sim");
    assert_eq!(list["connected"], Value::Null);

    let c = s.connect(&json!({"driver": "multicuber", "port": "sim"}));
    assert_eq!(c["ok"], true, "{}", c);
    assert_eq!(c["size"], 3);
    assert_eq!(c["info"]["proto"], "1");
    assert_eq!(s.list(true)["connected"]["driver"], "multicuber");
    assert_eq!(s.list(true)["connected"]["size"], 3);

    let mut since = 0;
    let started = s.scan(Some(3));
    assert_eq!(started["ok"], true, "{}", started);
    let run = started["run"].as_str().unwrap().to_string();
    let scan = read_until(&s, &mut since, &["scan-done", "error"], true, Duration::from_secs(60));
    assert_eq!(scan.last().unwrap()["type"], "scan-done", "{:?}", scan);
    let start = of_type(&scan, "scan-start");
    assert_eq!(start.len(), 1);
    let steps = start[0]["steps"].as_u64().unwrap() as usize;
    assert!(start[0]["eta_ms"].as_f64().unwrap() > 3000.0);
    // Six pictures, numbered in order, each of another face, each with a quarter turn and the time to the next.
    let photos = of_type(&scan, "photo");
    assert_eq!(photos.iter().map(|e| e["index"].as_u64().unwrap()).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5]);
    let faces: std::collections::BTreeSet<&str> = photos.iter().map(|e| e["face"].as_str().unwrap()).collect();
    assert_eq!(faces.into_iter().collect::<String>(), "BDFLRU");
    for (k, e) in photos.iter().enumerate() {
        assert!(e["rotate"].as_u64().unwrap() % 90 == 0 && e["rotate"].as_u64().unwrap() < 360);
        let next_ms = e["next_ms"].as_f64().unwrap();
        assert!(if k < 5 { next_ms > 200.0 && next_ms < 3000.0 } else { next_ms < 50.0 }, "photo {}: next_ms {}", k, next_ms);
    }
    // The planner's scan is quicker than the 2018 one (6.2 s of motion on a 3x3x3).
    assert!(start[0]["eta_ms"].as_f64().unwrap() < 6000.0);
    assert_eq!(of_type(&scan, "op").len(), steps, "one op event per command");
    for e in &scan {
        assert!(e["seq"].as_u64().is_some() && e["t_ms"].as_f64().is_some());
        if e["type"] != "log" {
            assert_eq!(e["run"], run.as_str());
        }
    }
    let seqs: Vec<u64> = scan.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "events are numbered one after the other");

    // The page analyses the pictures; here the simulator says what the cube
    // is, its colours named after the middle they belong with, as the page does.
    let (raw, solved, _, _) = sim_facelets(&s);
    // What the simulated camera saw at each picture (`sim`), turned as the
    // picture's `rotate` says, is that face of the cube the scan left.
    for e in &photos {
        let mut seen: Vec<u8> = e["sim"].as_str().expect("a simulated robot says what its camera sees").bytes().collect();
        assert_eq!(seen.len(), 9);
        for _ in 0..e["rotate"].as_u64().unwrap() / 90 {
            seen = (0..9).map(|i| seen[(2 - i % 3) * 3 + i / 3]).collect();
        }
        let face = "URFDLB".find(e["face"].as_str().unwrap()).unwrap();
        assert_eq!(std::str::from_utf8(&seen).unwrap(), &raw[face * 9..face * 9 + 9], "picture {}", e["face"]);
    }
    let g = anycube::cuben::Geometry::new(3);
    let facelets = g.to_string(&g.reframe(&g.from_string(&raw).unwrap(), &[]));
    assert!(!solved);
    let r = s.solve(&json!({"run": run, "size": 3, "facelets": facelets, "analysis_ms": 321}));
    assert_eq!(r["ok"], true, "{}", r);
    let solve = read_until(&s, &mut since, &["done", "error"], false, Duration::from_secs(120));
    assert_eq!(solve.last().unwrap()["type"], "done", "{:?}", solve);
    let plan = of_type(&solve, "plan");
    assert_eq!(plan.len(), 1);
    let plan = plan[0];
    assert_eq!(plan["metric"], "robot-ms");
    assert_eq!(plan["planner"], "optimized");
    assert!(plan["candidates"].as_u64().unwrap() >= 1);
    assert!(plan["compute_ms"].as_f64().unwrap() >= 0.0);
    let moves = plan["moves"].as_array().unwrap();
    assert!(!moves.is_empty() && moves.len() <= 24);
    let ops = plan["ops"].as_array().unwrap();
    assert_eq!(ops[0]["cmd"], "ENABLE");
    assert_eq!(ops[ops.len() - 1]["cmd"], "DISABLE");
    let eta: f64 = ops.iter().map(|o| o["est_ms"].as_f64().unwrap()).sum();
    assert!((eta - plan["eta_ms"].as_f64().unwrap()).abs() < 2.0);
    for o in ops {
        assert!(o["cmd"].is_string() && o["arg"].is_string() && o["est_ms"].is_number());
        assert!(o["move"].is_null() || o["move"].as_u64().unwrap() < moves.len() as u64);
    }
    // One move event per move, in order, each when its rotator turn starts.
    let move_events: Vec<u64> = of_type(&solve, "move").iter().map(|e| e["move"].as_u64().unwrap()).collect();
    assert_eq!(move_events, (0..moves.len() as u64).collect::<Vec<_>>());
    let op_events = of_type(&solve, "op");
    assert_eq!(op_events.len(), ops.len());
    for (k, e) in op_events.iter().enumerate() {
        assert_eq!(e["i"], k);
        assert_eq!(e["cmd"], ops[k]["cmd"]);
        assert_eq!(e["arg"], ops[k]["arg"]);
        assert!(e["actual_ms"].as_f64().unwrap() >= 0.0);
    }
    let done = solve.last().unwrap();
    for key in ["scan_ms", "analysis_ms", "compute_ms", "exec_ms", "total_ms"] {
        assert!(done["times"][key].is_number(), "{}", key);
    }
    assert_eq!(done["times"]["analysis_ms"], 321.0);
    assert!(done["times"]["total_ms"].as_f64().unwrap() >= done["times"]["exec_ms"].as_f64().unwrap());

    assert!(s.wait_idle(Duration::from_secs(5)));
    let (_, solved, overflows, violations) = sim_facelets(&s);
    assert!(solved, "the simulated cube is solved");
    assert_eq!(overflows, 0, "no byte lost to the receive buffer");
    assert!(violations.is_empty(), "{:?}", violations);
    // The firmware's order: drivers on before any motion, off at the end; the lift came down
    // last (no tipping back and forth after the last move).
    let log = sim_log(&s);
    assert_eq!(log.last().unwrap(), "DISABLE");
    assert_eq!(log[log.len() - 2], "LIFT 0");
    let note = read_until(&s, &mut since, &["log"], false, Duration::from_secs(5));
    assert!(note.last().unwrap()["text"].as_str().unwrap().contains("cube solved"), "{:?}", note);
    assert_eq!(s.disconnect()["ok"], true);
    assert_eq!(s.list(true)["connected"], Value::Null);
}

#[test]
fn rejected_scan_restores_exact_pose_on_firmware_22() {
    let s = session(0.01, 0);
    let connected = s.connect(&json!({"driver": "multicuber", "port": "sim"}));
    assert_eq!(connected["info"]["fw"], "2.2.0-sim");
    assert_eq!(connected["info"]["restore"], true);
    assert_eq!(connected["info"]["restore_after_scan"], true);
    let sim = s.with_robot(|r| r.as_any().downcast_ref::<MultiCuber>().unwrap().sim().unwrap().clone()).unwrap();
    // Begin away from the parked position so simply parking cannot pass.
    {
        let mut state = sim.state.0.lock().unwrap();
        state.cube.lift = anycube::robot::multicuber::planner::Lift::Level(3);
        state.cube.flip_up = false;
    }
    let pose = || {
        let state = sim.state.0.lock().unwrap();
        (state.cube.facelets.clone(), state.cube.lift, state.cube.flip_up, state.enabled)
    };
    let before = pose();
    let timeout = Duration::from_secs(8);
    let mut since = 0;
    let run = s.scan(Some(3))["run"].clone();
    for index in 0..3 {
        let photo = read_until(&s, &mut since, &["photo"], false, timeout).pop().unwrap();
        assert_eq!(photo["index"], index);
        if index == 2 { assert_ne!(pose().0, before.0); }
        assert_eq!(s.photo_taken(&json!({"run": run, "index": index, "failed": index == 2}))["ok"], true);
    }
    let error = read_until(&s, &mut since, &["error"], false, timeout).pop().unwrap();
    assert!(s.wait_idle(timeout));
    assert_eq!(pose(), before, "failed photo");
    assert_eq!(error["code"], "scan_restored");

    let run = s.scan(Some(3))["run"].clone();
    read_until(&s, &mut since, &["scan-done"], true, timeout);
    assert_ne!(pose(), before);
    assert_eq!(s.reject_scan(&json!({"run": "stale"}))["error"]["code"], "bad_run");
    let invalid = s.solve(&json!({"run": run, "facelets": "U".repeat(54)}));
    assert_eq!(invalid["ok"], false);
    assert!(s.wait_idle(timeout));
    assert_eq!(pose(), before, "invalid cube");
    assert_eq!(s.reject_scan(&json!({"run": run}))["error"]["code"], "bad_run");
    assert!(sim.violations().is_empty());
    assert_eq!(sim.overflows(), 0);
}

#[test]
fn a_stop_ends_the_run_and_switches_the_motors_off() {
    let s = session(0.2, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    let mut since = 0;
    let run = s.scan(Some(3))["run"].as_str().unwrap().to_string();
    // Let it get going, then stop.
    let _ = read_until(&s, &mut since, &["op"], true, Duration::from_secs(10));
    let t = Instant::now();
    assert_eq!(s.stop()["ok"], true);
    let events = read_until(&s, &mut since, &["error"], true, Duration::from_secs(10));
    let e = events.last().unwrap();
    assert_eq!((e["code"].as_str(), e["run"].as_str()), (Some("stopped"), Some(run.as_str())));
    assert!(t.elapsed() < Duration::from_secs(2));
    assert!(s.wait_idle(Duration::from_secs(5)));
    assert_eq!(sim_log(&s).last().unwrap(), "DISABLE");
    // A new scan needs no reconnect: the robot says where it is first.
    let again = s.scan(Some(3));
    assert_eq!(again["ok"], true, "{}", again);
    let done = read_until(&s, &mut since, &["scan-done", "error"], true, Duration::from_secs(60));
    assert_eq!(done.last().unwrap()["type"], "scan-done", "{:?}", done);
    assert!(sim_facelets(&s).3.is_empty());
}

#[test]
fn a_photo_not_taken_stops_the_scan() {
    let s = session(0.0, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    let mut since = 0;
    s.scan(Some(3));
    let t = Instant::now();
    let events = read_until(&s, &mut since, &["error"], false, Duration::from_secs(10));
    assert_eq!(events.last().unwrap()["code"], "photo_timeout");
    assert!(t.elapsed() >= Duration::from_millis(2900));
    assert!(s.wait_idle(Duration::from_secs(5)));
    assert_eq!(sim_log(&s).last().unwrap(), "DISABLE");
}

/// At real speed: with the next command sent ahead the robot does not wait
/// for the serial round trip between commands, so each takes what the
/// model says and the run what the plan says.
#[test]
fn commands_sent_ahead_leave_no_gaps() {
    let s = session(1.0, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    let mut since = 0;
    let (facelets, _, _, _) = sim_facelets(&s);
    // A solve without a scan starts its own run.
    let r = s.solve(&json!({"facelets": facelets, "analysis_ms": 0}));
    assert_eq!(r["ok"], true, "{}", r);
    let plan = read_until(&s, &mut since, &["plan", "error"], false, Duration::from_secs(60));
    let plan = plan.last().unwrap().clone();
    assert_eq!(plan["type"], "plan", "{}", plan);
    // Stop after a handful of commands: enough to see the gaps, a few seconds at most.
    let mut ops = Vec::new();
    let t0 = Instant::now();
    while ops.len() < 8 {
        let got = s.events(since, Duration::from_secs(2));
        since = got["seq"].as_u64().unwrap();
        ops.extend(got["events"].as_array().unwrap().iter().filter(|e| e["type"] == "op").cloned());
    }
    let elapsed = t0.elapsed().as_secs_f64() * 1000.0;
    s.stop();
    assert!(s.wait_idle(Duration::from_secs(5)));
    let est: Vec<f64> = plan["ops"].as_array().unwrap().iter().map(|o| o["est_ms"].as_f64().unwrap()).collect();
    let mut gaps = 0.0;
    for e in &ops[1..8] {
        let i = e["i"].as_u64().unwrap() as usize;
        let actual = e["actual_ms"].as_f64().unwrap();
        assert!((actual - est[i]).abs() < 25.0, "op {}: {} ms, model {} ms", i, actual, est[i]);
        gaps += actual - est[i];
    }
    let planned: f64 = est[..8].iter().sum();
    assert!(elapsed < planned + 150.0, "8 commands took {:.0} ms, the plan says {:.0} ms", elapsed, planned);
    assert_eq!(sim_facelets(&s).2, 0);
    eprintln!("8 commands: {:.0} ms against {:.0} ms planned; mean gap {:.1} ms", elapsed, planned, gaps / 7.0);
}

/// The search for candidates given no time at all (as when it has run out before the first
/// solution: cold tables, a slow machine): every solve still gets a plan and solves the cube.
#[test]
fn a_starved_search_never_fails_a_solve() {
    tables();
    let dir = std::env::temp_dir().join(format!("anycube-robot-session-{}-starved", std::process::id()));
    let search = anycube::robot::fastest::Options { plain_ms: 0.0, search_ms: 0.0, total_ms: 0.0, ..Default::default() };
    let s = Session::new(Config { settings_dir: Some(dir), sim_scale: Some(0.0), max_size: 50, photo_settle: Duration::ZERO, search });
    let g = anycube::cuben::Geometry::new(3);
    let mut since = 0;
    for round in 0..12 {
        // Each connect is a new simulated robot with a new scramble.
        assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
        let (raw, _, _, _) = sim_facelets(&s);
        let facelets = g.to_string(&g.reframe(&g.from_string(&raw).unwrap(), &[]));
        let r = s.solve(&json!({"size": 3, "facelets": facelets, "analysis_ms": 0}));
        assert_eq!(r["ok"], true, "{}", r);
        let events = read_until(&s, &mut since, &["done", "error"], false, Duration::from_secs(120));
        assert_eq!(events.last().unwrap()["type"], "done", "round {}: {}", round, events.last().unwrap());
        let plan = of_type(&events, "plan")[0];
        assert!(plan["candidates"].as_u64().unwrap() >= 1 && !plan["moves"].as_array().unwrap().is_empty());
        // The plain solver's answer is said with the plan, and the plan is never slower than it.
        assert!(plan["plain"]["moves"].as_u64().unwrap() >= 1 && plan["est_ms"].as_f64().unwrap() <= plan["plain"]["est_ms"].as_f64().unwrap());
        assert!(s.wait_idle(Duration::from_secs(5)));
        let (_, solved, _, violations) = sim_facelets(&s);
        assert!(solved && violations.is_empty(), "round {}: {:?}", round, violations);
    }
    assert_eq!(s.disconnect()["ok"], true);
}

fn sim_of(s: &Session) -> SimRobot {
    s.with_robot(|r| r.as_any().downcast_ref::<MultiCuber>().unwrap().sim().unwrap().clone()).unwrap()
}

/// The cable pulled while the robot runs: the run ends with `disconnected`, the connection
/// is dropped, a `disconnected` event says so, and the page can go back to watching.
#[test]
fn a_robot_pulled_out_while_it_runs_ends_the_run_and_the_connection() {
    let s = session(0.3, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    let sim = sim_of(&s);
    let mut since = 0;
    let run = s.scan(Some(3))["run"].as_str().unwrap().to_string();
    let _ = read_until(&s, &mut since, &["op"], true, Duration::from_secs(10));
    let t = Instant::now();
    sim.unplug();
    let events = read_until(&s, &mut since, &["disconnected"], true, Duration::from_secs(10));
    assert!(t.elapsed() < Duration::from_secs(3), "noticed after {:?}", t.elapsed());
    let errors = of_type(&events, "error");
    assert_eq!(errors.len(), 1, "{:?}", events);
    assert_eq!((errors[0]["code"].as_str(), errors[0]["run"].as_str()), (Some("disconnected"), Some(run.as_str())));
    let last = events.last().unwrap();
    assert!(last["run"].is_null() && last["message"].is_string(), "{}", last);
    // By then no robot is connected, and nothing can be started.
    assert_eq!(s.list(false)["connected"], Value::Null);
    assert!(s.wait_idle(Duration::from_secs(5)));
    assert_eq!(s.scan(Some(3))["error"]["code"], "not_connected");
    // Plugged in again (the simulated port is always there): it connects anew.
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    assert_eq!(s.disconnect()["ok"], true);
}

#[test]
fn a_robot_pulled_out_while_it_rests_is_noticed_too() {
    let s = session(0.0, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    let mut since = s.events(0, Duration::ZERO)["seq"].as_u64().unwrap();
    let t = Instant::now();
    sim_of(&s).unplug();
    let events = read_until(&s, &mut since, &["disconnected"], false, Duration::from_secs(10));
    assert!(t.elapsed() < Duration::from_secs(2), "noticed after {:?}", t.elapsed());
    assert!(of_type(&events, "error").is_empty(), "no run, no error: {:?}", events);
    assert_eq!(s.list(false)["connected"], Value::Null);
    assert_eq!(s.scan(Some(3))["error"]["code"], "not_connected");
}

/// `GET /api/robots` with boards plugged in: what each port carries, as API.md has it.
#[test]
fn the_list_says_what_each_port_carries() {
    tables();
    let port = |path: &str| PortId { path: path.into(), usb: Some((0x2341, 0x0042)), serial: Some(path.into()) };
    let detector = Detector::new(
        Box::new(move || vec![port("/dev/cu.usbmodem1"), port("/dev/cu.usbmodem2"), port("/dev/cu.usbmodem3")]),
        Box::new(|_| vec!["multicuber"]),
        Box::new(|_, path| match path {
            "/dev/cu.usbmodem1" => Probe::Robot(json!({"fw": "2.2.0", "proto": "1"})),
            "/dev/cu.usbmodem2" => Probe::NotThis,
            _ => Probe::Busy,
        }),
    );
    let config = Config { settings_dir: None, sim_scale: None, max_size: 50, photo_settle: Duration::ZERO, search: Default::default() };
    let s = Session::with_detector(config, detector.clone());
    // At once, before any port was asked: every port is there, none with a `driver` yet.
    let first = s.list(true);
    assert_eq!(first["drivers"][0]["ports"], json!(["/dev/cu.usbmodem1", "/dev/cu.usbmodem2", "/dev/cu.usbmodem3"]));
    assert_eq!(first["ports"].as_array().unwrap().len(), 3);
    assert!(detector.settle(Duration::from_secs(5)));
    let list = s.list(true);
    assert_eq!(
        list["ports"],
        json!([
            {"path": "/dev/cu.usbmodem1", "driver": "multicuber", "busy": false, "info": {"fw": "2.2.0", "proto": "1"}},
            {"path": "/dev/cu.usbmodem2", "driver": null, "busy": false},
            {"path": "/dev/cu.usbmodem3", "busy": true},
        ])
    );
    assert_eq!(list["connected"], Value::Null);
    // Without a port the connect goes to the board that answered as this robot (which is not
    // really there: the port cannot be opened).
    let c = s.connect(&json!({"driver": "multicuber"}));
    assert_eq!(c["error"]["code"], "port", "{}", c);
    assert!(c["error"]["message"].as_str().unwrap().contains("/dev/cu.usbmodem1"), "{}", c);
}

#[test]
fn each_scan_configures_the_same_22_connection_before_motion() {
    let s = session(0.0, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["size"], 3);
    let mut since = 0;
    for size in [None, Some(3), Some(4), Some(6), Some(5)] {
        let before = sim_log(&s).len();
        assert_eq!(s.scan(size)["ok"], true);
        let events = read_until(&s, &mut since, &["scan-done", "error"], true, Duration::from_secs(10));
        assert!(s.wait_idle(Duration::from_secs(1)));
        let log = sim_log(&s);
        if size.is_none() || size == Some(6) {
            assert_eq!(events.last().unwrap()["code"], "bad_size");
            assert_eq!(log.len(), before);
            continue;
        }
        let size = size.unwrap();
        assert_eq!(events.last().unwrap()["type"], "scan-done");
        assert_eq!(log[before..before + 4], [format!("SIZE {size}"), "CAL".into(), "STATE".into(), "ENABLE".into()]);
        assert_eq!(s.list(false)["connected"]["size"], size);
        let photos = of_type(&events, "photo");
        assert_eq!(photos.len(), 6);
        assert!(photos.iter().all(|photo| photo["sim"].as_str().unwrap().len() == size * size));
        assert!(sim_facelets(&s).3.is_empty());
    }
}

#[test]
fn size_photo_holds_then_restores_before_retry_and_scan() {
    let s = session(0.02, 60);
    let connected = s.connect(&json!({"driver":"multicuber","port":"sim"}));
    assert_eq!(connected["info"]["size_photo"], true);
    let sim = s.with_robot(|r| r.as_any().downcast_ref::<MultiCuber>().unwrap().sim().unwrap().clone()).unwrap();
    let pose = || {
        let state = sim.state.0.lock().unwrap();
        (state.cube.facelets.clone(), state.cube.lift, state.cube.flip_up, state.enabled)
    };
    let before = pose();
    let mut since = 0;
    let timeout = Duration::from_secs(8);
    for reading in [Value::Null, json!(3)] {
        let run = s.scan_with_size_photo()["run"].clone();
        let event = read_until(&s, &mut since, &["size-photo", "error"], false, timeout).pop().unwrap();
        assert_eq!(event["type"], "size-photo", "{event}");
        assert_eq!(event["run"], run);
        assert!(event["t_ms"].as_f64().unwrap() >= 60.0);
        assert!(!pose().2);
        assert_ne!(pose().0, before.0);
        assert_eq!(sim.log().last().unwrap(), "FLIP DOWN NORMAL");
        assert_eq!(s.measured_size(&json!({"run":"old", "size":3}))["ok"], false);
        assert_eq!(s.measured_size(&json!({"run":run, "size":reading}))["ok"], true);
        if reading.is_null() {
            let error = read_until(&s, &mut since, &["error"], false, timeout).pop().unwrap();
            assert_eq!(error["code"], "no_size");
            assert!(s.wait_idle(timeout));
            assert_eq!(pose(), before);
            assert!(sim.log().iter().all(|l| !l.starts_with("LIFT") && !l.starts_with("ROT")));
        } else {
            read_until(&s, &mut since, &["scan-done"], true, timeout);
            assert_eq!(s.reject_scan(&json!({"run":run}))["ok"], true);
            assert_eq!(pose(), before);
        }
    }
    assert!(sim.violations().is_empty());
}

#[test]
fn size_photo_refuses_a_raised_lift_and_stop_does_not_reverse() {
    let s = session(0.01, 0);
    s.connect(&json!({"driver":"multicuber","port":"sim"}));
    let sim = s.with_robot(|r| r.as_any().downcast_ref::<MultiCuber>().unwrap().sim().unwrap().clone()).unwrap();
    let timeout = Duration::from_secs(5);
    let mut since = 0;
    sim.state.0.lock().unwrap().cube.lift = anycube::robot::multicuber::planner::Lift::Scan;
    s.scan_with_size_photo();
    let error = read_until(&s, &mut since, &["error"], false, timeout).pop().unwrap();
    assert_eq!(error["code"], "position");
    assert!(s.wait_idle(timeout));
    assert!(!sim.log().iter().any(|line| line == "ENABLE"));
    sim.state.0.lock().unwrap().cube.lift = anycube::robot::multicuber::planner::Lift::Level(0);
    s.scan_with_size_photo();
    read_until(&s, &mut since, &["size-photo"], false, timeout);
    s.stop();
    assert!(s.wait_idle(timeout));
    assert_eq!(sim.log().last().unwrap(), "DISABLE");
    assert!(!sim.log().iter().any(|line| line.starts_with("FLIP UP")));
}
