//! Bounded, controller-planned solution transport. See docs/custom-robots.md.

use super::{ConnectOptions, Driver, PhotoSpec, Plan, PlannedOp, PortId, Probe, Progress, Robot, RobotError, StopFlag, Turn, FACES};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const ID: &str = "solution-link";
const MAX_MOVES: usize = 4096;
const MAX_IMAGE: usize = 256 * 1024; // base64 bytes
const MAX_LINE: usize = 128;
const IO_TICK: Duration = Duration::from_millis(25);
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const MOTION_TIMEOUT: Duration = Duration::from_secs(60);

pub fn endpoint() -> Option<String> {
    let executable = std::env::var("ANYCUBE_SOLUTION_LINK_CONTROLLER").ok()?;
    Path::new(&executable).is_absolute().then_some(executable)
}

pub struct SolutionDriver;

impl Driver for SolutionDriver {
    fn id(&self) -> &'static str { ID }
    fn name(&self) -> &'static str { "Custom controller (Solution Link)" }
    fn sizes(&self) -> Vec<usize> { Vec::new() }
    fn claims(&self, _: &PortId) -> bool { false }
    fn probe(&self, _: &str) -> Probe { Probe::NotThis }

    fn connect(&self, port: &str, size: Option<usize>, _: &ConnectOptions) -> Result<Box<dyn Robot>, RobotError> {
        if endpoint().as_deref() != Some(port) {
            return Err(RobotError::new("no_port", "set ANYCUBE_SOLUTION_LINK_CONTROLLER to the absolute path of a local executable"));
        }
        let robot = SolutionLink::handshake(Wire::spawn(port)?, port, size)?;
        Ok(Box::new(robot))
    }
}

fn protocol(message: impl Into<String>) -> RobotError { RobotError::new("protocol", message) }
fn link_error(e: std::io::Error) -> RobotError { RobotError::new("link", e.to_string()) }

fn check_stop(stop: &StopFlag) -> Result<(), RobotError> {
    if stop.is_set() { Err(RobotError::new("stopped", "stopped")) } else { Ok(()) }
}

struct Wire {
    child: Option<Child>,
    input: Option<ChildStdin>,
    lines: Option<Receiver<Result<String, RobotError>>>,
    present: Arc<AtomicBool>,
}

impl Wire {
    fn spawn(executable: &str) -> Result<Self, RobotError> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(link_error)?;
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (send, lines) = mpsc::sync_channel(1);
        let present = Arc::new(AtomicBool::new(true));
        let live = present.clone();
        std::thread::spawn(move || {
            let mut output = BufReader::new(output);
            loop {
                match read_record(&mut output) {
                    Ok(Some(line)) => {
                        if send.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = send.send(Err(error));
                        break;
                    }
                }
            }
            live.store(false, Ordering::SeqCst);
        });
        Ok(Self { child: Some(child), input, lines: Some(lines), present })
    }

    fn send(&mut self, line: &str, stop: &StopFlag) -> Result<(), RobotError> {
        check_stop(stop)?;
        let input = self.input.as_mut().ok_or_else(|| RobotError::new("link", "controller input is closed"))?;
        // One short record per reply, plus STOP. A compliant peer never fills this pipe.
        input.write_all(format!("{line}\n").as_bytes()).map_err(link_error)
    }

    fn read(&self, stop: &StopFlag, deadline: Instant) -> Result<String, RobotError> {
        let lines = self.lines.as_ref().ok_or_else(|| RobotError::new("link", "controller output is closed"))?;
        loop {
            check_stop(stop)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RobotError::new("timeout", "controller reply timed out"));
            }
            match lines.recv_timeout(remaining.min(IO_TICK)) {
                Ok(record) => {
                    check_stop(stop)?;
                    let line = record?;
                    if let Some(message) = line.strip_prefix("ERR ") {
                        return Err(RobotError::new("controller_rejected", message));
                    }
                    return Ok(line);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(RobotError::new("link", "controller output closed")),
            }
        }
    }

    fn expect(&mut self, expected: &str, stop: &StopFlag, timeout: Duration) -> Result<(), RobotError> {
        let reply = self.read(stop, Instant::now() + timeout)?;
        if reply != expected {
            return Err(protocol(format!("expected {expected}, received {reply}")));
        }
        Ok(())
    }

    fn close(&mut self) {
        self.input.take(); // EOF tells a controller to abort and exit.
        self.lines.take();
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        return;
                    }
                    std::thread::sleep(IO_TICK);
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        }
        self.present.store(false, Ordering::SeqCst);
    }
}

impl Drop for Wire {
    fn drop(&mut self) {
        self.close();
    }
}

fn read_record(reader: &mut impl BufRead) -> Result<Option<String>, RobotError> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().map_err(link_error)?;
        if bytes.is_empty() {
            return if line.is_empty() { Ok(None) } else { Err(protocol("incomplete record from controller")) };
        }
        let end = bytes.iter().position(|&b| b == b'\n');
        let len = end.unwrap_or(bytes.len());
        if line.len() + len > MAX_LINE || !bytes[..len].iter().all(|b| (b' '..=b'~').contains(b)) {
            return Err(protocol("expected an ASCII line of at most 128 bytes"));
        }
        line.extend_from_slice(&bytes[..len]);
        reader.consume(len + usize::from(end.is_some()));
        if end.is_some() {
            return Ok(Some(String::from_utf8(line).unwrap()));
        }
    }

}

struct SolutionLink {
    link: Wire,
    port: String,
    n: usize,
    width: usize,
    half: bool,
    png: bool,
    ms: u32,
    closed: bool,
}

impl SolutionLink {
    fn handshake(mut link: Wire, port: &str, size: Option<usize>) -> Result<Self, RobotError> {
        let stop = StopFlag::default();
        link.send("HELLO 1", &stop)?;
        let hello = link.read(&stop, Instant::now() + REPLY_TIMEOUT)?;
        let fields: Vec<&str> = hello.split_ascii_whitespace().collect();
        let ["SOLUTION", "1", source @ ("app" | "png"), n, width, half @ ("0" | "1"), ms] = fields.as_slice() else {
            return Err(protocol("expected SOLUTION 1 <app|png> <size> <width> <half> <ms>"));
        };
        let n = n.parse::<usize>().ok().filter(|n| (2..=50).contains(n)).ok_or_else(|| protocol("invalid cube size"))?;
        let width = width.parse::<usize>().ok().filter(|&w| (1..n).contains(&w)).ok_or_else(|| protocol("invalid maximum turn width"))?;
        let ms = ms.parse::<u32>().ok().filter(|&ms| ms <= 60_000).ok_or_else(|| protocol("invalid move estimate"))?;
        if size.is_some_and(|size| size != n) {
            return Err(RobotError::new("bad_size", format!("controller is set for {n}×{n}×{n}")));
        }
        Ok(Self { link, port: port.into(), n, width, half: *half == "1", png: *source == "png", ms, closed: false })
    }

    fn scan(&mut self, stop: &StopFlag, progress: &mut dyn FnMut(Progress) -> Result<(), RobotError>) -> Result<(), RobotError> {
        self.link.send("SCAN", stop)?;
        self.link.expect("OK", stop, REPLY_TIMEOUT)?;
        let mut faces = [false; 6];
        for i in 0..6 {
            let reply = self.link.read(stop, Instant::now() + MOTION_TIMEOUT)?;
            let fields: Vec<&str> = reply.split_ascii_whitespace().collect();
            let ["FACE", index, face, rotate] = fields.as_slice() else { return Err(protocol("expected FACE <index> <face> <rotate>")); };
            let Some(f) = face.as_bytes().first().and_then(|first| FACES.iter().position(|b| b == first)).filter(|_| face.len() == 1) else {
                return Err(protocol("invalid face label"));
            };
            if *index != i.to_string() || faces[f] {
                return Err(protocol("out-of-order index or duplicate face label"));
            }
            faces[f] = true;
            let rotate = rotate.parse::<u16>().ok().filter(|r| [0, 90, 180, 270].contains(r)).ok_or_else(|| protocol("invalid frame rotation"))?;
            let png = if self.png { Some(self.image(stop)?) } else { None };
            progress(Progress::Frame { i, photo: PhotoSpec { index: i, face: FACES[f] as char, rotate }, png })?;
            self.link.send(&format!("ACK {i}"), stop)?;
        }
        self.link.expect("DONE", stop, MOTION_TIMEOUT)
    }

    fn image(&mut self, stop: &StopFlag) -> Result<String, RobotError> {
        let deadline = Instant::now() + MOTION_TIMEOUT;
        let mut image = String::new();
        loop {
            let reply = self.link.read(stop, deadline)?;
            if reply == "END" && !image.is_empty() && image.len() % 4 == 0 {
                return Ok(image);
            }
            let data = reply.strip_prefix("DATA ").filter(|data| !data.is_empty()).ok_or_else(|| protocol("expected PNG DATA or END"))?;
            if image.len() + data.len() > MAX_IMAGE || !data.bytes().all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)) {
                return Err(protocol("invalid or oversized PNG payload"));
            }
            image.push_str(data);
        }
    }

    fn run_solution(&mut self, plan: &Plan, stop: &StopFlag, progress: &mut dyn FnMut(Progress) -> Result<(), RobotError>) -> Result<(), RobotError> {
        if plan.ops.is_empty() {
            return check_stop(stop);
        }
        self.link.send(&format!("SOLVE {}", plan.ops.len()), stop)?;
        self.link.expect("OK", stop, REPLY_TIMEOUT)?;
        for op in &plan.ops {
            self.link.send(&format!("MOVE {}", op.arg), stop)?;
            self.link.expect("OK", stop, REPLY_TIMEOUT)?;
        }
        self.link.send("RUN", stop)?;
        self.link.expect("ACCEPT", stop, REPLY_TIMEOUT)?;
        for i in 0..plan.ops.len() {
            let began = Instant::now();
            self.link.expect(&format!("MOVED {i}"), stop, MOTION_TIMEOUT)?;
            progress(Progress::MoveDone { i, actual_ms: began.elapsed().as_secs_f64() * 1000.0 })?;
        }
        self.link.expect("DONE", stop, MOTION_TIMEOUT)
    }
}

impl Robot for SolutionLink {
    fn driver_id(&self) -> &'static str { ID }
    fn port(&self) -> String { self.port.clone() }
    fn size(&self) -> usize { self.n }
    fn info(&self) -> Value {
        json!({"protocol": 1, "camera": if self.png { "robot" } else { "app" }, "max_width": self.width, "half": self.half, "move_ms": self.ms})
    }
    fn sync(&mut self) -> Result<(), RobotError> {
        if self.closed { Err(RobotError::new("link", "reconnect the controller before starting again")) } else { Ok(()) }
    }
    fn plan_scan(&self) -> Plan {
        Plan {
            ops: (0..6).map(|_| PlannedOp { cmd: "FACE".into(), arg: String::new(), est_ms: 0.0, mv: None, photo: None }).collect(),
            eta_ms: 0.0,
        }
    }
    fn plan_solution(&self, moves: &[Turn]) -> Plan {
        Plan {
            ops: moves.iter().enumerate().map(|(i, mv)| PlannedOp { cmd: "MOVE".into(), arg: mv.name(), est_ms: self.ms.into(), mv: Some(i), photo: None }).collect(),
            eta_ms: moves.len() as f64 * f64::from(self.ms),
        }
    }
    fn check_solution(&self, moves: &[Turn]) -> Result<(), RobotError> {
        check_moves(moves, self.width, self.half)
    }

    fn estimator(&self) -> Box<dyn Fn(&[Turn]) -> f64 + Send> {
        let ms = f64::from(self.ms);
        Box::new(move |moves| moves.len() as f64 * ms)
    }
    fn controller_planned(&self) -> bool { true }
    fn planner_name(&self) -> &'static str { ID }
    fn presence(&self) -> Arc<dyn Fn() -> bool + Send + Sync> {
        let present = self.link.present.clone();
        Arc::new(move || present.load(Ordering::SeqCst))
    }
    fn connection_closed(&self) -> bool { self.closed }
    fn execute(&mut self, plan: &Plan, stop: &StopFlag, progress: &mut dyn FnMut(Progress) -> Result<(), RobotError>) -> Result<(), RobotError> {
        self.sync()?;
        let result = if plan.ops.first().is_some_and(|op| op.cmd == "FACE") {
            self.scan(stop, progress)
        } else {
            self.run_solution(plan, stop, progress)
        };
        let result = result.and_then(|()| check_stop(stop));
        if result.is_err() {
            self.emergency_stop();
        }
        result
    }
    fn emergency_stop(&mut self) {
        if !self.closed {
            let _ = self.link.send("STOP", &StopFlag::default());
            self.close();
        }
    }
    fn park(&mut self) -> Result<(), RobotError> { Ok(()) }
    fn close(&mut self) {
        self.closed = true;
        self.link.close();
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

fn check_moves(moves: &[Turn], width: usize, half: bool) -> Result<(), RobotError> {
    if moves.len() > MAX_MOVES {
        return Err(RobotError::new("unsupported_solution", "controller protocol supports up to 4096 moves"));
    }
    for turn in moves {
        if !matches!(*turn, Turn::Layers { depth, turns, .. } if (1..=width).contains(&usize::from(depth)) && (matches!(turns, -1 | 1) || turns == 2 && half)) {
            return Err(RobotError::new("unsupported_move", format!("controller cannot execute {}", turn.name())));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_wide_and_half_moves_before_staging() {
        let moves = Turn::parse_all("R Uw2 F' 3Rw", 4).unwrap();
        assert!(check_moves(&moves, 3, true).is_ok());
        assert_eq!(check_moves(&moves, 2, true).unwrap_err().message, "controller cannot execute 3Rw");
        assert_eq!(check_moves(&moves, 3, false).unwrap_err().message, "controller cannot execute Uw2");
        let rotation = Turn::parse_all("x", 4).unwrap();
        assert_eq!(check_moves(&rotation, 3, true).unwrap_err().code, "unsupported_move");
    }

    #[test]
    fn records_are_bounded_and_lf_terminated() {
        let mut lines = &b"OK\nMOVED 0\n"[..];
        assert_eq!(read_record(&mut lines).unwrap().as_deref(), Some("OK"));
        assert_eq!(read_record(&mut lines).unwrap().as_deref(), Some("MOVED 0"));
        assert_eq!(read_record(&mut lines).unwrap(), None);
        for mut malformed in [&[b'x'; 129][..], &b"OK\r\n"[..], &b"unterminated"[..]] {
            assert_eq!(read_record(&mut malformed).unwrap_err().code, "protocol");
        }
    }
}
