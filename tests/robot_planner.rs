//! The Rust port of MultiCuber's planner makes exactly the commands of
//! robot.py's `Planner`: the same lines, the same remaps (how the cube
//! turned) and the same pictures, for random solutions of every size from
//! every starting state, for scans, and for a whole session (scan, solve,
//! park on one planner). The fixtures come from robot.py itself
//! (`scripts/robot_fixtures.py`).
#![cfg(feature = "robots")]

use anycube::robot::multicuber::planner::{BRule, Lift, Op, Planner, Remap};
use anycube::robot::Turn;
use serde_json::Value;

fn text(op: &Op) -> String {
    let mut s = op.line();
    if op.remap != Remap::None {
        s += " @";
        s += op.remap.name();
    }
    if let Some(p) = op.photo {
        s += &format!(" #{}:{}", p.face, p.rotate);
    }
    s
}

fn texts(ops: &[Op]) -> Vec<String> {
    ops.iter().map(text).collect()
}

fn want(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

fn lift(s: &str) -> Lift {
    Lift::parse(s).unwrap()
}

#[test]
fn planner_matches_robot_py() {
    let data: Value = serde_json::from_str(include_str!("fixtures/multicuber_planner.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    let mut checked = 0;
    let mut ops_checked = 0;
    for (k, c) in cases.iter().enumerate() {
        let n = c["n"].as_u64().unwrap() as usize;
        let moves = Turn::parse_all(c["moves"].as_str().unwrap_or(""), n).unwrap();
        let rule = if c["b_rule"] == "flipper" { BRule::Flipper } else { BRule::Legacy };
        match c["kind"].as_str().unwrap() {
            "solution" => {
                let mut p = Planner::new(n, rule);
                p.flip_up = c["flip_up"].as_bool().unwrap();
                p.lift = lift(c["lift"].as_str().unwrap());
                p.solution(&moves);
                assert_eq!(texts(&p.ops), want(&c["ops"]), "case {}: {}x{}x{} {}", k, n, n, n, c["moves"]);
                assert_eq!(p.lift, lift(c["end"]["lift"].as_str().unwrap()));
                assert_eq!(p.flip_up, c["end"]["flip_up"].as_bool().unwrap());
                ops_checked += p.ops.len();
            }
            "scan" => {
                let mut p = Planner::new(n, BRule::Legacy);
                p.flip_up = c["flip_up"].as_bool().unwrap();
                p.scan(c["scan_flip_normal"].as_bool().unwrap());
                assert_eq!(texts(&p.ops), want(&c["ops"]), "case {}: scan of {}", k, n);
                assert_eq!(p.flip_up, c["end"]["flip_up"].as_bool().unwrap());
                ops_checked += p.ops.len();
            }
            "session" => {
                let mut p = Planner::new(n, BRule::Legacy);
                p.scan(false);
                assert_eq!(texts(&std::mem::take(&mut p.ops)), want(&c["scan"]), "case {}", k);
                p.solution(&moves);
                assert_eq!(texts(&std::mem::take(&mut p.ops)), want(&c["solve"]), "case {}", k);
                p.park();
                assert_eq!(texts(&p.ops), want(&c["park"]), "case {}", k);
            }
            "plan_solution" => {
                // robot.plan_solution: the flipper is DOWN after a scan.
                let mut p = Planner::new(n, BRule::Legacy);
                p.flip_up = false;
                p.solution(&moves);
                assert_eq!(texts(&p.ops), want(&c["ops"]), "case {}", k);
            }
            other => panic!("unknown case kind {}", other),
        }
        checked += 1;
    }
    assert!(checked >= 200 && ops_checked > 5000, "{} cases, {} commands", checked, ops_checked);
}

#[test]
fn notation_round_trips() {
    for (t, n) in [("R", 3), ("U'", 3), ("Fw2", 4), ("3Rw'", 6), ("y2", 5), ("x", 3), ("B2", 3)] {
        assert_eq!(Turn::parse(t, n).unwrap().name(), t);
    }
    for (bad, n) in [("3Rw", 3), ("4Rw", 4), ("M", 3), ("2R", 4), ("R3", 3), ("r", 4)] {
        assert!(Turn::parse(bad, n).is_err(), "{}", bad);
    }
    // anycube's move numbers: (face + 6 * (layers - 1)) * 3 + power.
    assert_eq!(Turn::from_index(0).name(), "U");
    assert_eq!(Turn::from_index(4).name(), "R2");
    assert_eq!(Turn::from_index(8).name(), "F'");
    assert_eq!(Turn::from_index(18 + 3).name(), "Rw");
}
