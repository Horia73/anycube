//! The local server behind the interface.
//!
//! It serves the page's files (built into the program) and a small JSON
//! API on 127.0.0.1, for one person on one machine: no limits on size or
//! number of solves beyond one search at a time.
//!
//! ```text
//! POST   /api/solve        {"size": 4, "moves": "Rw U2 F'", "mode": "fast"}   -> {"id": ..., "status": ...}
//! GET    /api/solve/{id}   where the solve stands; `?since=<epoch>&wait=10` holds the answer until something changes
//! DELETE /api/solve/{id}   stop it
//! POST   /api/check        {"size": 3, "facelets": "..."}                     -> {"ok": true} or what is wrong
//! GET    /api/scramble     ?size=3: a random-state scramble, for the sizes that have one
//! GET    /api/config       what this server does
//! POST   /api/settings     {"top_colour": "white" | "yellow"}
//! ```
//!
//! A search cannot be interrupted from inside, so the solvers run in a
//! child process (this program again, as `app --worker`) that keeps each
//! size's tables loaded, takes one job at a time on its standard input and
//! answers on its standard output. Stopping a solve kills the child; the
//! next solve starts a new one. What the child writes to its standard
//! error while it builds tables is passed on to the page, so that the
//! minutes a first solve of a size takes are not spent in front of a
//! silent screen.

use super::assets;
use crate::api::{self, check, solve, Input, Mode};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

/// Lines of the worker's protocol start with this, so that nothing else a
/// library might print is mistaken for one.
const MARK: &str = "@anycube ";
/// The longest a status request is held open waiting for news.
const MAX_WAIT: Duration = Duration::from_secs(10);
/// How long a finished solve is kept for the page to collect.
const KEEP: Duration = Duration::from_secs(600);

enum State {
    Waiting,
    Running { since: Instant, loading: bool },
    Finished { outcome: Value, at: Instant, waited_ms: u64 },
}

struct Job {
    request: Value,
    state: State,
    submitted: Instant,
}

#[derive(Default)]
struct Board {
    jobs: HashMap<String, Job>,
    order: VecDeque<String>,
    running: Option<String>,
    /// Goes up whenever anything changes; held requests wake on it.
    epoch: u64,
    /// The last line the worker wrote while loading or building tables.
    progress: String,
    count: u64,
}

struct Shared {
    board: Mutex<Board>,
    changed: Condvar,
    /// The worker process, where a request to stop a solve can reach it.
    child: Mutex<Option<Child>>,
    settings: Mutex<PathBuf>,
    max_size: usize,
}

impl Shared {
    fn bump(&self, board: &mut Board) {
        board.epoch += 1;
        self.changed.notify_all();
    }
}

fn failed(code: &str, message: &str) -> Value {
    json!({"result": "failed", "code": code, "message": message})
}

// ---------------------------------------------------------------- worker

/// Checks and solves one request in this process.
fn solve_here(request: &Value, max_size: usize, started: impl FnOnce()) -> Value {
    let size = request["size"].as_u64().unwrap_or(0) as usize;
    let Some(mode) = Mode::parse(request["mode"].as_str().unwrap_or("normal")) else {
        return failed("bad_mode", "the mode is fast or normal");
    };
    let text = request["input"].as_str().unwrap_or("");
    let input = if request["kind"] == "moves" { Input::Moves(text) } else { Input::Facelets(text) };
    let cube = match check(size, input, max_size) {
        Ok(c) => c,
        Err(e) => return failed(e.code, &e.message),
    };
    api::solve::load(size);
    started();
    let t = Instant::now();
    match solve(&cube, mode) {
        Ok(s) => json!({
            "result": "solved",
            "solution": s.solution,
            "length": s.moves.len(),
            "parts": s.parts.iter().map(|&(name, moves)| json!({"name": name, "moves": moves})).collect::<Vec<_>>(),
            "optimal": s.optimal,
            "search_ms": t.elapsed().as_millis() as u64,
        }),
        Err(e) => failed(e.code, &e.message),
    }
}

/// The worker's side: jobs in, events out, until the server closes the pipe.
pub fn worker() {
    let say = |event: Value| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{}{}", MARK, event);
        let _ = out.flush();
    };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(job) = serde_json::from_str::<Value>(&line) else { continue };
        let max_size = job["max_size"].as_u64().unwrap_or(0) as usize;
        let outcome = solve_here(&job, max_size, || say(json!({"event": "started"})));
        say(json!({"event": "done", "outcome": outcome}));
    }
}

/// Starts the worker. Its standard error is read line by line into the
/// board (and passed on to this program's own).
fn spawn_worker(shared: &Arc<Shared>) -> std::io::Result<(std::process::ChildStdin, BufReader<std::process::ChildStdout>)> {
    let mut child = Command::new(std::env::current_exe()?)
        .args(["app", "--worker"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take().expect("piped");
    let stdout = BufReader::new(child.stdout.take().expect("piped"));
    let stderr = child.stderr.take().expect("piped");
    *shared.child.lock().unwrap() = Some(child);
    let shared = shared.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            eprintln!("{}", line);
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let mut board = shared.board.lock().unwrap();
            board.progress = text.chars().take(160).collect();
            shared.bump(&mut board);
        }
    });
    Ok((stdin, stdout))
}

/// Takes jobs off the board one at a time and runs them on the worker.
fn run_jobs(shared: Arc<Shared>) {
    let mut pipes: Option<(std::process::ChildStdin, BufReader<std::process::ChildStdout>)> = None;
    loop {
        // The next job, when there is one.
        let (id, mut request) = {
            let mut board = shared.board.lock().unwrap();
            loop {
                let now = Instant::now();
                board.jobs.retain(|_, j| !matches!(&j.state, State::Finished { at, .. } if now.duration_since(*at) > KEEP));
                if let Some(id) = board.order.pop_front() {
                    let Some(job) = board.jobs.get_mut(&id) else { continue };
                    job.state = State::Running { since: now, loading: true };
                    let request = job.request.clone();
                    board.running = Some(id.clone());
                    board.progress.clear();
                    shared.bump(&mut board);
                    break (id, request);
                }
                board = shared.changed.wait(board).unwrap();
            }
        };
        request["max_size"] = json!(shared.max_size);
        if pipes.is_none() {
            pipes = spawn_worker(&shared).ok();
        }
        let outcome = match pipes.as_mut() {
            None => failed("solver_unavailable", "the solver process could not be started"),
            Some((stdin, stdout)) => {
                let sent = writeln!(stdin, "{}", request).and_then(|_| stdin.flush()).is_ok();
                let mut outcome = None;
                let mut line = String::new();
                while sent && outcome.is_none() {
                    line.clear();
                    if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let Some(event) = line.strip_prefix(MARK).and_then(|l| serde_json::from_str::<Value>(l).ok()) else { continue };
                    if event["event"] == "started" {
                        let mut board = shared.board.lock().unwrap();
                        if let Some(Job { state: State::Running { since, loading }, .. }) = board.jobs.get_mut(&id) {
                            (*since, *loading) = (Instant::now(), false);
                        }
                        board.progress.clear();
                        shared.bump(&mut board);
                    } else if event["event"] == "done" {
                        outcome = Some(event["outcome"].clone());
                    }
                }
                outcome
                    .unwrap_or_else(|| failed("stopped", "the solve was stopped, or the solver stopped on this cube"))
            }
        };
        // A worker that did not answer is gone (stopped, or crashed): the next job gets a new one.
        if outcome["code"] == "stopped" || outcome["code"] == "solver_unavailable" {
            pipes = None;
            if let Some(mut child) = shared.child.lock().unwrap().take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        let mut board = shared.board.lock().unwrap();
        board.running = None;
        board.progress.clear();
        let now = Instant::now();
        if let Some(job) = board.jobs.get_mut(&id) {
            let waited_ms = now.duration_since(job.submitted).as_millis() as u64;
            job.state = State::Finished { outcome, at: now, waited_ms };
        }
        shared.bump(&mut board);
    }
}

// ------------------------------------------------------------------ http

fn view(board: &Board, id: &str) -> Option<Value> {
    let job = board.jobs.get(id)?;
    let mut v = match &job.state {
        State::Waiting => {
            let position = board.order.iter().position(|x| x == id).map_or(board.order.len(), |p| p + 1);
            json!({"status": "queued", "position": position, "waiting": board.order.len()})
        }
        State::Running { since, loading } => json!({
            "status": "running",
            "phase": if *loading { "loading" } else { "searching" },
            "for_ms": since.elapsed().as_millis() as u64,
            "progress": board.progress,
        }),
        State::Finished { outcome, waited_ms, .. } if outcome["result"] == "solved" => json!({
            "status": "done",
            "solution": outcome["solution"],
            "length": outcome["length"],
            "parts": outcome["parts"],
            "optimal": outcome["optimal"],
            "time_ms": outcome["search_ms"],
            "waited_ms": waited_ms,
        }),
        State::Finished { outcome, .. } => json!({
            "status": "failed",
            "error": {"code": outcome["code"], "message": outcome["message"], "impossible": false},
        }),
    };
    v["id"] = json!(id);
    v["epoch"] = json!(board.epoch);
    v["size"] = job.request["size"].clone();
    v["mode"] = job.request["mode"].clone();
    Some(v)
}

fn error(code: &str, message: impl Into<String>, impossible: bool) -> Value {
    json!({"error": {"code": code, "message": message.into(), "impossible": impossible}})
}

/// A request body as a cube to check: its size, which kind of input, the input.
fn cube_of(body: &Value) -> Result<(usize, &'static str, String), Value> {
    let (kind, input) = match (body["moves"].as_str(), body["facelets"].as_str()) {
        (Some(m), None) => ("moves", m.to_string()),
        (None, Some(f)) => ("facelets", f.to_string()),
        _ => return Err(error("bad_request", "send either moves or facelets", false)),
    };
    let size = match body["size"].as_u64() {
        Some(n) => n as usize,
        // Facelets say their own size: six squares.
        None if kind == "facelets" => (2..=api::HARD_MAX_SIZE)
            .find(|n| 6 * n * n == input.trim().len())
            .ok_or_else(|| error("bad_facelets", "the facelets are not six squares", false))?,
        None => return Err(error("bad_size", "a scramble needs the size of the cube", false)),
    };
    Ok((size, kind, input))
}

fn checked(size: usize, kind: &str, input: &str, max_size: usize) -> Result<(), Value> {
    let input = if kind == "moves" { Input::Moves(input) } else { Input::Facelets(input) };
    check(size, input, max_size).map(|_| ()).map_err(|e| error(e.code, e.message.clone(), e.impossible()))
}

fn query(url: &str, name: &str) -> Option<u64> {
    url.split_once('?')?.1.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('=')?.parse().ok())
}

const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data: blob:; \
                   connect-src 'self' https://api.github.com/repos/Horia73/anycube/releases/latest; \
                   worker-src 'self'; media-src 'self' blob:; object-src 'none'; base-uri 'none'; \
                   form-action 'self'; frame-ancestors 'none'";

fn respond(request: Request, status: u16, mime: &str, body: Vec<u8>, cache: &str) {
    let header = |name: &str, value: &str| Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("a plain header");
    let response = Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", mime))
        .with_header(header("Cache-Control", cache))
        .with_header(header("Content-Security-Policy", CSP))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Referrer-Policy", "no-referrer"));
    let _ = request.respond(response);
}

fn respond_json(request: Request, status: u16, body: Value) {
    respond(request, status, "application/json", body.to_string().into_bytes(), "no-store");
}

fn read_settings(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn write_top_colour(path: &Path, colour: &str) -> std::io::Result<()> {
    let mut settings = read_settings(path);
    settings["top_colour"] = json!(colour);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, settings.to_string())?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

#[cfg(test)]
mod settings_tests {
    use super::{fs, json, read_settings, write_top_colour};

    #[test]
    fn orientation_is_replaced_without_losing_other_settings() {
        let path = std::env::temp_dir().join(format!("anycube-settings-test-{}.json", std::process::id()));
        fs::write(&path, r#"{"other":true}"#).unwrap();
        write_top_colour(&path, "yellow").unwrap();
        assert_eq!(read_settings(&path), json!({"top_colour": "yellow", "other": true}));
        write_top_colour(&path, "white").unwrap();
        assert_eq!(read_settings(&path), json!({"top_colour": "white", "other": true}));
        fs::remove_file(path).unwrap();
    }
}

fn handle(shared: &Arc<Shared>, port: u16, mut request: Request) {
    // Only this machine's own pages may talk to the server: a page elsewhere
    // that points a name at 127.0.0.1 arrives with that name as its host.
    let host = request.headers().iter().find(|h| h.field.equiv("Host")).map(|h| h.value.to_string()).unwrap_or_default();
    if host != format!("127.0.0.1:{}", port) && host != format!("localhost:{}", port) {
        return respond_json(request, 403, error("forbidden", "this server answers on 127.0.0.1 only", false));
    }
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("").to_string();
    let method = request.method().clone();
    let mut body = String::new();
    if matches!(method, Method::Post) {
        let _ = request.as_reader().take(256 * 1024).read_to_string(&mut body);
    }
    let parsed = || serde_json::from_str::<Value>(&body).map_err(|_| error("bad_request", "the body is not JSON", false));

    match (&method, path.as_str()) {
        (Method::Get, "/api/config") => respond_json(
            request,
            200,
            json!({
                "max_size": shared.max_size,
                "browser_sizes": api::BROWSER_SIZES,
                "modes": ["fast", "normal"],
                "queue": 0,
                "time_cap_s": 0,
                "max_scramble_moves": api::MAX_SCRAMBLE_MOVES,
                "repo_url": null,
                "local": true,
                "top_colour": if read_settings(&shared.settings.lock().unwrap())["top_colour"] == "yellow" { "yellow" } else { "white" },
                "version": env!("CARGO_PKG_VERSION"),
            }),
        ),
        (Method::Post, "/api/settings") => {
            let origin = format!("http://{}", host);
            if request.headers().iter().any(|h| h.field.equiv("Origin") && h.value.as_str() != origin) {
                return respond_json(request, 403, error("forbidden", "settings requests must come from this app", false));
            }
            let json_body = request.headers().iter().any(|h| {
                h.field.equiv("Content-Type") && h.value.as_str().split(';').next().is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
            });
            if !json_body {
                return respond_json(request, 415, error("bad_request", "settings must be sent as JSON", false));
            }
            let Ok(settings) = parsed() else {
                return respond_json(request, 400, error("bad_request", "the body is not JSON", false));
            };
            let Some(colour @ ("white" | "yellow")) = settings["top_colour"].as_str() else {
                return respond_json(request, 400, error("bad_request", "top_colour must be white or yellow", false));
            };
            if write_top_colour(&shared.settings.lock().unwrap(), colour).is_err() {
                return respond_json(request, 500, error("settings", "could not save settings", false));
            }
            respond_json(request, 200, json!({"top_colour": colour}))
        }
        (Method::Get, "/api/health") => {
            let board = shared.board.lock().unwrap();
            respond_json(request, 200, json!({"status": "ok", "waiting": board.order.len(), "busy": board.running.is_some()}))
        }
        (Method::Get, "/api/scramble") => {
            let size = query(&url, "size").unwrap_or(0) as usize;
            // The seed: what the standard library seeds its own hash tables with.
            use std::hash::{BuildHasher, Hasher};
            let seed = std::collections::hash_map::RandomState::new().build_hasher().finish();
            respond_json(request, 200, json!({"scramble": api::random_state(size, seed)}))
        }
        (Method::Post, "/api/check") => {
            let answer = parsed().and_then(|b| cube_of(&b)).and_then(|(size, kind, input)| checked(size, kind, &input, shared.max_size));
            match answer {
                Ok(()) => respond_json(request, 200, json!({"ok": true})),
                Err(e) => respond_json(request, 200, json!({"ok": false, "error": e["error"]})),
            }
        }
        (Method::Post, "/api/solve") => {
            let job = parsed().and_then(|b| {
                let (size, kind, input) = cube_of(&b)?;
                let mode = b["mode"].as_str().unwrap_or("normal").to_string();
                if Mode::parse(&mode).is_none() {
                    return Err(error("bad_mode", "the mode is fast or normal", false));
                }
                checked(size, kind, &input, shared.max_size)?;
                Ok(json!({"size": size, "kind": kind, "input": input, "mode": mode}))
            });
            match job {
                Err(e) => respond_json(request, 400, e),
                Ok(job) => {
                    let mut board = shared.board.lock().unwrap();
                    board.count += 1;
                    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
                    let id = format!("{:x}{:04x}", stamp, board.count);
                    board.jobs.insert(id.clone(), Job { request: job, state: State::Waiting, submitted: Instant::now() });
                    board.order.push_back(id.clone());
                    shared.bump(&mut board);
                    let v = view(&board, &id).expect("just added");
                    drop(board);
                    respond_json(request, 202, v)
                }
            }
        }
        (Method::Get, p) if p.starts_with("/api/solve/") => {
            let id = &p["/api/solve/".len()..];
            let since = query(&url, "since");
            let deadline = Instant::now() + Duration::from_secs(query(&url, "wait").unwrap_or(0)).min(MAX_WAIT);
            let mut board = shared.board.lock().unwrap();
            loop {
                let Some(v) = view(&board, id) else {
                    drop(board);
                    return respond_json(request, 404, error("not_found", "no such solve: it was stopped, or its result is no longer kept", false));
                };
                let news = since != Some(board.epoch) || v["status"] == "done" || v["status"] == "failed";
                let left = deadline.saturating_duration_since(Instant::now());
                if news || left.is_zero() {
                    drop(board);
                    return respond_json(request, 200, v);
                }
                board = shared.changed.wait_timeout(board, left).unwrap().0;
            }
        }
        (Method::Delete, p) if p.starts_with("/api/solve/") => {
            let id = &p["/api/solve/".len()..];
            let mut board = shared.board.lock().unwrap();
            let running = board.running.as_deref() == Some(id);
            board.order.retain(|x| x != id);
            board.jobs.remove(id);
            shared.bump(&mut board);
            drop(board);
            if running {
                // The search itself cannot be told to stop: its process is.
                if let Some(child) = shared.child.lock().unwrap().as_mut() {
                    let _ = child.kill();
                }
            }
            respond_json(request, 200, json!({"id": id, "status": "cancelled"}))
        }
        (_, p) if p.starts_with("/api/") => respond_json(request, 404, error("not_found", "no such endpoint", false)),
        (Method::Get, p) => match assets::find(p.trim_start_matches('/')) {
            Some(a) => respond(request, 200, a.mime, a.bytes.to_vec(), "no-cache"),
            None => respond(request, 404, "text/plain; charset=utf-8", b"not found\n".to_vec(), "no-store"),
        },
        _ => respond_json(request, 405, error("bad_request", "not something this server does", false)),
    }
}

/// A server that has started: where it is, and the thread that serves.
pub struct Running {
    pub url: String,
    pub port: u16,
}

/// Starts the server on 127.0.0.1, on `port` or on any free port for 0, and
/// returns once it is listening. It serves until the program ends.
pub fn start(port: u16, max_size: usize) -> Result<Running, String> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("cannot listen on 127.0.0.1:{}: {}", port, e))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).ok_or("no port")?;
    let shared = Arc::new(Shared {
        board: Mutex::default(), changed: Condvar::new(), child: Mutex::new(None),
        settings: Mutex::new(super::data_dir().join("settings.json")), max_size,
    });
    let jobs = shared.clone();
    std::thread::spawn(move || run_jobs(jobs));
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let shared = shared.clone();
            // A held status request keeps its thread for up to ten seconds: one thread each.
            std::thread::spawn(move || handle(&shared, port, request));
        }
    });
    Ok(Running { url: format!("http://127.0.0.1:{}/", port), port })
}
