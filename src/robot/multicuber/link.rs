//! The serial line to MultiCuber's firmware: lines out, `OK`/`ERR` lines back.

use crate::robot::RobotError;
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

/// Bytes to and from a robot.
pub trait Transport: Send {
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    /// What arrives within `timeout` (0 bytes when nothing does).
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize>;
    /// Forget what has arrived and was not read.
    fn clear_input(&mut self) {}
}

/// A serial port, through the `serialport` crate.
pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialTransport {
    pub fn into_port(self) -> Box<dyn serialport::SerialPort> { self.port }

    pub fn from_port(port: Box<dyn serialport::SerialPort>) -> Self { Self { port } }

    pub fn open(path: &str, baud: u32) -> Result<SerialTransport, RobotError> {
        let port = serialport::new(path, baud)
            .dtr_on_open(false)
            .timeout(Duration::from_millis(20))
            .open()
            .map_err(|e| RobotError::new("port", format!("cannot open {}: {}", path, e)))?;
        Ok(SerialTransport { port })
    }
}

impl Transport for SerialTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.port.write_all(data)?;
        self.port.flush()
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        let _ = self.port.set_timeout(timeout.max(Duration::from_millis(1)));
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => Ok(0),
            Err(e) => Err(e),
        }
    }

    fn clear_input(&mut self) {
        let _ = self.port.clear(serialport::ClearBuffer::Input);
    }
}

/// What came back for a command.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok(String),
    Err(String),
}

/// Lines over a transport. Lines starting with `#` (information) are kept in `info`.
pub struct Link {
    t: Box<dyn Transport>,
    partial: Vec<u8>,
    lines: std::collections::VecDeque<String>,
    pub info: Vec<String>,
}

impl Link {
    pub fn new(t: Box<dyn Transport>) -> Link {
        Link { t, partial: Vec::new(), lines: Default::default(), info: Vec::new() }
    }

    pub fn send(&mut self, line: &str) -> Result<(), RobotError> {
        self.t
            .write(format!("{}\n", line.trim()).as_bytes())
            .map_err(|e| RobotError::new("link", format!("the robot's port failed: {}", e)))
    }

    /// The next line that is not information, or `None` if none came by `deadline`.
    pub fn read_reply(&mut self, deadline: Instant) -> Result<Option<Reply>, RobotError> {
        loop {
            while let Some(line) = self.lines.pop_front() {
                let text = line.trim();
                if text.is_empty() {
                    continue;
                }
                if text.starts_with('#') || text.starts_with('@') {
                    self.info.push(text.to_string());
                    if self.info.len() > 200 {
                        self.info.drain(..100);
                    }
                    continue;
                }
                if let Some(rest) = text.strip_prefix("OK") {
                    return Ok(Some(Reply::Ok(rest.trim().to_string())));
                }
                if let Some(rest) = text.strip_prefix("ERR") {
                    return Ok(Some(Reply::Err(rest.trim().to_string())));
                }
                return Err(RobotError::new("protocol", format!("unexpected answer from the robot: {:?}", text)));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            let mut buf = [0u8; 256];
            let got = self
                .t
                .read(&mut buf, (deadline - now).min(Duration::from_millis(50)))
                .map_err(|e| RobotError::new("link", format!("the robot's port failed: {}", e)))?;
            for &b in &buf[..got] {
                if b == b'\n' {
                    let line = String::from_utf8_lossy(&self.partial).replace('\r', "");
                    self.partial.clear();
                    self.lines.push_back(line);
                } else if self.partial.len() < 512 {
                    self.partial.push(b);
                }
            }
        }
    }

    /// One command, waiting for its answer.
    pub fn command(&mut self, line: &str, timeout: Duration) -> Result<String, RobotError> {
        self.send(line)?;
        match self.read_reply(Instant::now() + timeout)? {
            Some(Reply::Ok(rest)) => Ok(rest),
            Some(Reply::Err(rest)) => Err(RobotError::new("robot_refused", format!("{} refused: {}", line, rest))),
            None => Err(RobotError::new("timeout", format!("no answer to {} in {:.0} s", line, timeout.as_secs_f64()))),
        }
    }

    /// Opening the port restarts the Mega: wait for its `# MULTICUBER ... ready`
    /// so that nothing is sent while the bootloader listens.
    pub fn wait_banner(&mut self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            let before = self.info.len();
            if self.read_reply(Instant::now() + Duration::from_millis(50)).is_err() {
                continue;
            }
            if self.info[before.min(self.info.len())..].iter().any(|l| l.starts_with("# MULTICUBER")) {
                return true;
            }
        }
        false
    }

    pub fn clear_input(&mut self) {
        self.t.clear_input();
        self.partial.clear();
        self.lines.clear();
    }

    /// HELLO until it answers; the fields of the answer.
    pub fn handshake(&mut self, attempts: usize, proto: u32) -> Result<BTreeMap<String, String>, RobotError> {
        let mut last = None;
        for _ in 0..attempts {
            match self.command("HELLO", Duration::from_secs(2)) {
                Ok(reply) => {
                    let fields: BTreeMap<String, String> = reply
                        .split_whitespace()
                        .skip(1)
                        .filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
                        .collect();
                    if !reply.starts_with("MULTICUBER") || fields.get("proto").and_then(|p| p.parse::<u32>().ok()) != Some(proto) {
                        return Err(RobotError::new("wrong_firmware", format!("not MultiCuber firmware with protocol {}: {:?}", proto, reply)));
                    }
                    return Ok(fields);
                }
                Err(e) if e.code == "timeout" || e.code == "protocol" => {
                    last = Some(e);
                    self.clear_input();
                }
                Err(e) => return Err(e),
            }
        }
        Err(RobotError::new(
            "no_answer",
            format!("the robot does not answer HELLO ({}); is firmware 2 on the board?", last.map_or(String::new(), |e| e.message)),
        ))
    }
}

/// `key=value` fields of a reply such as STATE's.
pub fn fields(reply: &str) -> BTreeMap<String, String> {
    reply.split_whitespace().filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))).collect()
}
