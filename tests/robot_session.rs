//! A whole run on the simulated MultiCuber through the session the server
//! uses: connect, scan (the pictures acknowledged as the page does), solve
//! from the facelets of the cube the scan left, and the events as
//! robots-spec/API.md lists them. The simulated cube ends solved, no byte
//! is lost to the robot's receive buffer, and nothing unsafe is done.
#![cfg(feature = "robots")]

use anycube::robot::multicuber::MultiCuber;
use anycube::robot::session::{Config, Session};
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
    let list = s.list();
    assert_eq!(list["drivers"][0]["id"], "multicuber");
    assert_eq!(list["drivers"][0]["ports"][0], "sim");
    assert_eq!(list["connected"], Value::Null);

    let c = s.connect(&json!({"driver": "multicuber", "port": "sim"}));
    assert_eq!(c["ok"], true, "{}", c);
    assert_eq!(c["size"], 3);
    assert_eq!(c["info"]["proto"], "1");
    assert_eq!(s.list()["connected"]["driver"], "multicuber");
    assert_eq!(s.list()["connected"]["size"], 3);

    let mut since = 0;
    let started = s.scan();
    assert_eq!(started["ok"], true, "{}", started);
    let run = started["run"].as_str().unwrap().to_string();
    let scan = read_until(&s, &mut since, &["scan-done", "error"], true, Duration::from_secs(60));
    assert_eq!(scan.last().unwrap()["type"], "scan-done", "{:?}", scan);
    let start = of_type(&scan, "scan-start");
    assert_eq!(start.len(), 1);
    let steps = start[0]["steps"].as_u64().unwrap() as usize;
    assert!(start[0]["eta_ms"].as_f64().unwrap() > 3000.0);
    let photos: Vec<(String, u64, u64)> = of_type(&scan, "photo")
        .iter()
        .map(|e| (e["face"].as_str().unwrap().to_string(), e["rotate"].as_u64().unwrap(), e["index"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        photos,
        vec![("U".into(), 270, 0), ("R".into(), 180, 1), ("D".into(), 90, 2), ("L".into(), 0, 3), ("B".into(), 0, 4), ("F".into(), 0, 5)]
    );
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
    // The firmware's order: drivers on before any motion, off at the end, parked.
    let log = sim_log(&s);
    assert_eq!(log.last().unwrap(), "DISABLE");
    let note = read_until(&s, &mut since, &["log"], false, Duration::from_secs(5));
    assert!(note.last().unwrap()["text"].as_str().unwrap().contains("cube solved"), "{:?}", note);
    assert_eq!(s.disconnect()["ok"], true);
    assert_eq!(s.list()["connected"], Value::Null);
}

#[test]
fn a_stop_ends_the_run_and_switches_the_motors_off() {
    let s = session(0.2, 0);
    assert_eq!(s.connect(&json!({"driver": "multicuber", "port": "sim"}))["ok"], true);
    let mut since = 0;
    let run = s.scan()["run"].as_str().unwrap().to_string();
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
    let again = s.scan();
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
    s.scan();
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
