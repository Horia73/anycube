//! The robot API of the local server (robots-spec/API.md): requests to the
//! robot session, answers as JSON.
//!
//! ```text
//! GET  /api/robots                          drivers, their sizes and ports, what each port carries, the connected robot
//! POST /api/robot/connect                   {"driver": "multicuber", "port": "...", "size": 3} (port and size optional)
//! POST /api/robot/disconnect                {}
//! POST /api/robot/scan                      {"size": 3}                                   -> {"ok": true, "run": "..."}
//! POST /api/robot/photo-taken               {"run": "...", "index": 0..5}
//! POST /api/robot/scan                      {"detect_size": true} for a MultiCuber size-photo hold
//! POST /api/robot/measured-size             {"run": "...", "size": 3} or null when the hold shows no face
//! POST /api/robot/reject-scan               {"run": "..."}                         return a rejected host-planned scan
//! POST /api/robot/solve                     {"run": "...", "size": 3, "facelets": "...", "analysis_ms": 412}
//! POST /api/robot/stop                      {}
//! GET  /api/robot/events?since=<seq>&wait=<s>
//! ```
//!
//! A command moves a machine, so a POST must come as JSON from this app's
//! own page: a page elsewhere cannot send a JSON body here without the
//! browser asking first, and this server never says yes.

use crate::robot::session::Session;
use serde_json::{json, Value};
use std::time::Duration;

/// Answers a robot request, or `None` if the path is not one.
pub fn route(session: &Session, method: &str, path: &str, url: &str, body: &str, json_body: bool, origin_ok: bool) -> Option<(u16, Value)> {
    let err = |status: u16, code: &str, message: &str| Some((status, json!({"ok": false, "error": {"code": code, "message": message}})));
    match (method, path) {
        // `?probe=0`: only what is known already; no port is opened to ask what is on it.
        ("GET", "/api/robots") => Some((200, session.list(super::server::query(url, "probe") != Some(0)))),
        ("GET", "/api/robot/events") => {
            let q = |name: &str| super::server::query(url, name);
            let since = q("since").unwrap_or(0);
            let wait = Duration::from_secs(q("wait").unwrap_or(0).min(10));
            Some((200, session.events(since, wait)))
        }
        ("POST", p) if p.starts_with("/api/robot/") => {
            if !origin_ok {
                return err(403, "forbidden", "robot commands must come from this app");
            }
            if !json_body {
                return err(415, "bad_request", "robot commands must be sent as JSON (Content-Type: application/json)");
            }
            let body: Value = if body.trim().is_empty() { json!({}) } else {
                match serde_json::from_str(body) {
                    Ok(v) => v,
                    Err(_) => return err(400, "bad_request", "the body is not JSON"),
                }
            };
            let answer = match p {
                "/api/robot/connect" => session.connect(&body),
                "/api/robot/disconnect" => session.disconnect(),
                "/api/robot/scan" if body["detect_size"] == true && body.get("size").is_none() => session.scan_with_size_photo(),
                "/api/robot/scan" if body["detect_size"] == true => return err(400, "bad_size", "a size-photo scan cannot also specify a size"),
                "/api/robot/measured-size" => session.measured_size(&body),
                "/api/robot/scan" => match body["size"].as_u64().and_then(|n| usize::try_from(n).ok()) {
                    Some(n) if n >= 2 => session.scan(Some(n)),
                    None if body.is_object() && body.get("size").is_none() => session.scan(None),
                    _ => return err(400, "bad_size", "send the size established before the scan"),
                },
                "/api/robot/photo-taken" => session.photo_taken(&body),
                "/api/robot/reject-scan" => session.reject_scan(&body),
                "/api/robot/solve" => session.solve(&body),
                "/api/robot/stop" => session.stop(),
                _ => return err(404, "not_found", "no such endpoint"),
            };
            Some((200, answer))
        }
        _ => None,
    }
}
