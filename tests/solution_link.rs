//! The app's normal session with a separate controller and pixel-based vision.
#![cfg(all(feature = "app", unix))]

use anycube::app::robots::route;
use anycube::robot::detect::Detector;
use anycube::robot::session::{Config, Session};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn post(session: &Session, action: &str, body: Value) -> Value {
    let path = format!("/api/robot/{action}");
    let (status, reply) = route(session, "POST", &path, &path, &body.to_string(), true, true).unwrap();
    assert_eq!(status, 200, "{reply}");
    reply
}

fn event_until(session: &Session, since: &mut u64, mut visit: impl FnMut(&Value) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for event after {since}");
        let batch = session.events(*since, Duration::from_millis(100));
        for event in batch["events"].as_array().unwrap() {
            *since = event["seq"].as_u64().unwrap();
            if visit(event) {
                return;
            }
        }
    }
}

fn fixture(root: &Path, mode: &str, flags: &str) {
    let bridge = root.join("controller");
    let log = root.join("controller.log");
    fs::write(
        &bridge,
        format!(
            "#!/bin/sh\ndir=$(dirname \"$0\")\nexec python3 scripts/solution_controller.py {flags} --frames-dir \"$dir\" 2>\"$dir/controller.log\"\n"
        ),
    ).unwrap();
    fs::set_permissions(&bridge, fs::Permissions::from_mode(0o700)).unwrap();
    let detector = Detector::new(Box::new(Vec::new), Box::new(|_| vec![]), Box::new(|_, _| unreachable!()));
    let session = Session::with_detector(Config {
        settings_dir: None,
        sim_scale: None,
        max_size: 50,
        photo_settle: Duration::ZERO,
        search: Default::default(),
    }, detector);
    let connect = |size| post(&session, "connect", json!({"driver": "solution-link", "size": size}));
    let (_, ports) = route(&session, "GET", "/api/robots", "/api/robots?probe=0", "", false, true).unwrap();
    assert!(ports["ports"].as_array().unwrap().iter().any(|p| p["driver"] == "solution-link" && p["manual"] == true));
    assert_eq!(connect(4)["error"]["code"], "bad_size");
    let connected = connect(3);
    assert_eq!(connected["ok"], true, "{connected}");
    assert_eq!(connected["info"]["camera"], if mode == "app" { "app" } else { "robot" });

    let mut vision = Command::new("node").arg("app/tests/solution-link-vision.mjs").arg(root)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut input = vision.stdin.take().unwrap();
    let mut output = BufReader::new(vision.stdout.take().unwrap());
    let mut analyse = |event: &Value| {
        writeln!(input, "{event}").unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        line.trim().to_string()
    };

    let scan = post(&session, "scan", json!({}));
    assert_eq!(scan["ok"], true, "{scan}");
    let run = &scan["run"];
    let mut since = 0;
    let mut photos = 0;
    event_until(&session, &mut since, |event| {
        assert_ne!(event["type"], "error", "{event}");
        if event["type"] == "photo" {
            assert_eq!(event["index"], photos);
            assert_eq!(event["png"].is_string(), mode != "app");
            if photos == 0 {
                let early = post(&session, "photo-taken", json!({"run": run, "index": 1}));
                assert_eq!(early["error"]["code"], "bad_photo", "{early}");
            }
            assert_eq!(analyse(event), "ACK");
            let ack = post(&session, "photo-taken", json!({"run": run, "index": event["index"]}));
            assert_eq!(ack["ok"], true, "{ack}");
            photos += 1;
        }
        event["type"] == "scan-done"
    });
    assert_eq!(photos, 6);
    let facelets = analyse(&json!({"type": "assemble"}));
    assert_eq!(facelets.len(), 54);
    drop(input);
    assert!(vision.wait().unwrap().success());
    let request = json!({"run": run, "size": 3, "facelets": facelets});

    let wrong = post(&session, "solve", json!({"run": run, "size": 4, "facelets": facelets}));
    assert_eq!(wrong["error"]["code"], "bad_size", "{wrong}");
    let invalid = post(&session, "solve", json!({"run": run, "size": 3, "facelets": "invalid"}));
    assert_eq!(invalid["ok"], false, "{invalid}");
    let response = post(&session, "solve", request.clone());
    assert_eq!(response["ok"], true, "{response}");
    let mut plan = false;
    let mut completed = 0;
    let mut terminal = Value::Null;
    event_until(&session, &mut since, |event| {
        if event["type"] == "plan" {
            assert_eq!(event["planner"], "solution-link");
            assert_eq!(event["moves"], json!(["R", "U2", "F'"]));
            plan = true;
        }
        if event["type"] == "move" {
            assert_eq!(event["move"], completed);
            assert_eq!(event["completed"], true);
            completed += 1;
            if mode == "stop" {
                assert_eq!(post(&session, "stop", json!({}))["ok"], true);
            }
        }
        if event["type"] == "done" || event["type"] == "error" {
            terminal = event.clone();
            return true;
        }
        false
    });
    assert_eq!(plan, mode != "half");
    match mode {
        "png" | "app" => {
            assert_eq!(terminal["type"], "done", "{terminal}");
            assert_eq!(completed, 3);
            assert_eq!(post(&session, "solve", request.clone())["error"]["code"], "bad_run");
        }
        "half" => assert_eq!(terminal["code"], "unsupported_move", "{terminal}"),
        "reject" => assert_eq!(terminal["code"], "controller_rejected", "{terminal}"),
        "stop" => assert_eq!(terminal["code"], "stopped", "{terminal}"),
        _ => unreachable!(),
    }
    if matches!(mode, "reject" | "stop") {
        event_until(&session, &mut since, |event| event["type"] == "disconnected");
        assert_eq!(post(&session, "solve", request.clone())["error"]["code"], "not_connected");
        assert_eq!(post(&session, "scan", json!({}))["error"]["code"], "not_connected");
    }
    assert_eq!(post(&session, "disconnect", json!({}))["ok"], true);
    let expected_moves = match mode {
        "png" | "app" => 3,
        "stop" => 1,
        _ => 0,
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    let output = loop {
        let output = fs::read_to_string(&log).unwrap();
        if output.contains("Link closed or stopped") || output.contains("Fixture solved: True") || mode == "reject" {
            break output;
        }
        assert!(Instant::now() < deadline, "{mode}: {output}");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(output.matches("actuator channel=").count(), expected_moves, "{mode}: {output}");
    if expected_moves == 3 {
        assert!(output.contains("Fixture solved: True"), "{output}");
    }
    eprintln!("{mode}: {terminal}\n{output}");
}

#[test]
#[ignore = "requires Python 3, Node.js, and AnyCube's 3x3x3 tables"]
fn controller_photos_app_camera_and_motion_failures() {
    let root = std::env::temp_dir().join(format!("anycube-solution-link-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    std::env::set_var("ANYCUBE_SOLUTION_LINK_CONTROLLER", root.join("controller"));
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
    for (mode, flags) in [
        ("png", "--delay 0"),
        ("app", "--camera app --delay 0"),
        ("half", "--no-half"),
        ("reject", "--reject"),
        ("stop", "--delay 0.5"),
    ] {
        fixture(&root, mode, flags);
    }
    fs::remove_dir_all(root).unwrap();
}
