//! Finding a robot when its board is plugged in, without anybody choosing a port.
//!
//! The serial ports that may carry a board are listed whenever somebody
//! asks (`ports`; the page asks every two seconds while no robot is
//! connected). A port seen for the first time is asked once what it is: the
//! drivers that claim its name try their handshake on it, on a thread of
//! their own, and the answer is kept for as long as the port stays there
//! with the same USB numbers. So the list is answered at once from what is
//! known, and a port is opened once, not on every poll: opening it restarts
//! an Arduino. A port that leaves is forgotten, and asked again when it
//! comes back. A port another program holds is `busy`; it is tried again
//! now and then, which opens nothing as long as it is held.
//!
//! A port that is being connected, or is connected, is never probed
//! (`reserve`); a connect waits for the probe of its port to end first, so
//! that the two never open it at once.

use super::{drivers, PortId, Probe};
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A busy port is tried again after this long.
const BUSY_RETRY: Duration = Duration::from_secs(6);
/// The longest a connect waits for the probe of its port.
const PROBE_WAIT: Duration = Duration::from_secs(15);

/// Serial ports that may carry a robot's board: USB serial ports by their
/// names (macOS `/dev/cu.usbmodem*`, `/dev/cu.usbserial*`,
/// `/dev/cu.wchusbserial*`; Linux `/dev/ttyACM*`, `/dev/ttyUSB*`; Windows
/// `COM*` of a USB device), Arduino's own first.
pub fn serial_ports() -> Vec<PortId> {
    let Ok(ports) = serialport::available_ports() else { return Vec::new() };
    let mut out: Vec<(u8, PortId)> = Vec::new();
    for p in ports {
        let path = p.port_name;
        let named = ["/dev/cu.usbmodem", "/dev/cu.usbserial", "/dev/cu.wchusbserial", "/dev/ttyACM", "/dev/ttyUSB"].iter().any(|prefix| path.starts_with(prefix));
        let (usb, serial) = match &p.port_type {
            serialport::SerialPortType::UsbPort(u) => (Some((u.vid, u.pid)), u.serial_number.clone()),
            _ => (None, None),
        };
        if !(named || (path.starts_with("COM") && usb.is_some())) {
            continue;
        }
        let rank = match usb {
            // Arduino (and its clone makers), then WCH CH340 and FTDI.
            Some((0x2341 | 0x2A03, _)) => 0,
            Some((0x1A86 | 0x0403, _)) => 1,
            _ => 2,
        };
        out.push((rank, PortId { path, usb, serial }));
    }
    out.sort_by(|a, b| (a.0, &a.1.path).cmp(&(b.0, &b.1.path)));
    out.into_iter().map(|(_, p)| p).collect()
}

/// What is known of a port.
#[derive(Clone, Debug, PartialEq)]
pub enum Known {
    /// Not asked yet, or being asked right now.
    Waiting,
    Probing,
    /// A driver's robot answered.
    Robot(&'static str, Value),
    /// A board that answered as no robot.
    Nothing,
    /// Another program holds the port.
    Busy,
}

#[derive(Clone, Debug)]
pub struct PortView {
    pub id: PortId,
    /// The drivers that claim the port by its name.
    pub claims: Vec<&'static str>,
    pub known: Known,
    /// When `known` was found.
    pub at: Instant,
}

type Enumerate = Box<dyn Fn() -> Vec<PortId> + Send + Sync>;
type Claims = Box<dyn Fn(&PortId) -> Vec<&'static str> + Send + Sync>;
type ProbeFn = Box<dyn Fn(&'static str, &str) -> Probe + Send + Sync>;

#[derive(Default)]
struct State {
    ports: Vec<PortView>,
    /// A thread is asking ports.
    working: bool,
    /// The port that is being connected or is connected.
    reserved: Option<String>,
}

struct Inner {
    state: Mutex<State>,
    changed: Condvar,
    enumerate: Enumerate,
    claims: Claims,
    probe: ProbeFn,
}

#[derive(Clone)]
pub struct Detector {
    inner: Arc<Inner>,
}

impl Detector {
    /// Over the computer's serial ports and the drivers anycube knows.
    pub fn system() -> Detector {
        Detector::new(
            Box::new(serial_ports),
            Box::new(|port| drivers().iter().filter(|d| d.claims(port)).map(|d| d.id()).collect()),
            Box::new(|driver, path| drivers().iter().find(|d| d.id() == driver).map_or(Probe::NotThis, |d| d.probe(path))),
        )
    }

    /// Over any list of ports and any way of asking (the tests' own).
    pub fn new(enumerate: Enumerate, claims: Claims, probe: ProbeFn) -> Detector {
        Detector { inner: Arc::new(Inner { state: Mutex::default(), changed: Condvar::new(), enumerate, claims, probe }) }
    }

    /// The ports that are there now, with what is known of each. With
    /// `probe`, the ports nobody has asked yet are asked, in the background.
    pub fn ports(&self, probe: bool) -> Vec<PortView> {
        let now: Vec<PortId> = (self.inner.enumerate)();
        let mut s = self.inner.state.lock().unwrap();
        // What left is forgotten; what is new waits to be asked.
        s.ports.retain(|p| now.contains(&p.id));
        for id in now {
            if !s.ports.iter().any(|p| p.id == id) {
                let claims = (self.inner.claims)(&id);
                let known = if claims.is_empty() { Known::Nothing } else { Known::Waiting };
                s.ports.push(PortView { id, claims, known, at: Instant::now() });
            }
        }
        if probe && !s.working && Self::next(&s).is_some() {
            s.working = true;
            let me = self.clone();
            std::thread::spawn(move || me.work());
        }
        s.ports.clone()
    }

    /// The next port to ask.
    fn next(s: &State) -> Option<usize> {
        s.ports.iter().position(|p| {
            s.reserved.as_deref() != Some(p.id.path.as_str())
                && match p.known {
                    Known::Waiting => true,
                    Known::Busy => p.at.elapsed() >= BUSY_RETRY,
                    _ => false,
                }
        })
    }

    fn work(&self) {
        loop {
            let (id, claims) = {
                let mut s = self.inner.state.lock().unwrap();
                let Some(i) = Self::next(&s) else {
                    s.working = false;
                    return;
                };
                s.ports[i].known = Known::Probing;
                (s.ports[i].id.clone(), s.ports[i].claims.clone())
            };
            let mut found = Known::Nothing;
            for driver in claims {
                match (self.inner.probe)(driver, &id.path) {
                    Probe::Robot(info) => found = Known::Robot(driver, info),
                    Probe::Busy => found = Known::Busy,
                    Probe::NotThis => continue,
                }
                break;
            }
            let mut s = self.inner.state.lock().unwrap();
            // Unless the port left meanwhile (or left and came back: then it waits to be asked anew).
            if let Some(p) = s.ports.iter_mut().find(|p| p.id == id && p.known == Known::Probing) {
                p.known = found;
                p.at = Instant::now();
            }
            self.inner.changed.notify_all();
        }
    }

    /// Keeps a port from being probed (it is being connected), waiting
    /// first for a probe of it that is under way. What is known of the
    /// port then, if it is listed. `None` gives the port free again.
    pub fn reserve(&self, path: Option<&str>) -> Option<PortView> {
        let mut s = self.inner.state.lock().unwrap();
        s.reserved = path.map(str::to_string);
        let path = path?;
        let deadline = Instant::now() + PROBE_WAIT;
        loop {
            let view = s.ports.iter().find(|p| p.id.path == path).cloned();
            let left = deadline.saturating_duration_since(Instant::now());
            if !view.as_ref().is_some_and(|v| v.known == Known::Probing) || left.is_zero() {
                return view;
            }
            s = self.inner.changed.wait_timeout(s, left).unwrap().0;
        }
    }

    /// A connect found out what a port is.
    pub fn identified(&self, path: &str, driver: &'static str, info: Value) {
        let mut s = self.inner.state.lock().unwrap();
        if let Some(p) = s.ports.iter_mut().find(|p| p.id.path == path) {
            p.known = Known::Robot(driver, info);
            p.at = Instant::now();
        }
    }

    /// Waits until no port is waiting to be asked or being asked (for tests).
    pub fn settle(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            {
                let s = self.inner.state.lock().unwrap();
                if !s.working && Self::next(&s).is_none() {
                    return true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn port(path: &str, serial: &str) -> PortId {
        PortId { path: path.into(), usb: Some((0x2341, 0x0042)), serial: Some(serial.into()) }
    }

    struct Bench {
        there: Arc<Mutex<Vec<PortId>>>,
        probes: Arc<Mutex<Vec<String>>>,
        held: Arc<AtomicUsize>,
        d: Detector,
    }

    /// Ports named `robot*` answer as the driver "r", `busy*` are held by another program
    /// while `held` is not 0, the others answer as nothing. A probe takes `ms`.
    fn bench(ms: u64) -> Bench {
        let there: Arc<Mutex<Vec<PortId>>> = Arc::default();
        let probes: Arc<Mutex<Vec<String>>> = Arc::default();
        let held = Arc::new(AtomicUsize::new(1));
        let (t, p, h) = (there.clone(), probes.clone(), held.clone());
        let d = Detector::new(
            Box::new(move || t.lock().unwrap().clone()),
            Box::new(|id| if id.path.contains("other") { vec![] } else { vec!["r"] }),
            Box::new(move |driver, path| {
                p.lock().unwrap().push(format!("{} {}", driver, path));
                std::thread::sleep(Duration::from_millis(ms));
                if path.contains("busy") && h.load(Ordering::SeqCst) != 0 {
                    Probe::Busy
                } else if path.contains("robot") || path.contains("busy") {
                    Probe::Robot(json!({"proto": "1"}))
                } else {
                    Probe::NotThis
                }
            }),
        );
        Bench { there, probes, held, d }
    }

    fn known(views: &[PortView], path: &str) -> Known {
        views.iter().find(|v| v.id.path == path).map(|v| v.known.clone()).unwrap()
    }

    #[test]
    fn a_port_is_asked_once_however_often_the_list_is_read() {
        let b = bench(30);
        *b.there.lock().unwrap() = vec![port("/dev/robot1", "A"), port("/dev/board", "B"), port("/dev/other", "C")];
        // At once, before anything is known.
        let t = Instant::now();
        let first = b.d.ports(true);
        assert!(t.elapsed() < Duration::from_millis(20));
        assert_eq!(first.len(), 3);
        assert!(matches!(known(&first, "/dev/robot1"), Known::Waiting | Known::Probing));
        // A port no driver claims is not opened at all.
        assert_eq!(known(&first, "/dev/other"), Known::Nothing);
        for _ in 0..20 {
            b.d.ports(true);
        }
        assert!(b.d.settle(Duration::from_secs(5)));
        let views = b.d.ports(true);
        assert_eq!(known(&views, "/dev/robot1"), Known::Robot("r", json!({"proto": "1"})));
        assert_eq!(known(&views, "/dev/board"), Known::Nothing);
        for _ in 0..20 {
            b.d.ports(true);
        }
        assert!(b.d.settle(Duration::from_secs(5)));
        assert_eq!(*b.probes.lock().unwrap(), vec!["r /dev/robot1".to_string(), "r /dev/board".to_string()]);
    }

    #[test]
    fn a_port_that_left_and_came_back_is_asked_again() {
        let b = bench(1);
        *b.there.lock().unwrap() = vec![port("/dev/robot1", "A")];
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        b.there.lock().unwrap().clear();
        assert!(b.d.ports(true).is_empty());
        // Another board on the same path between two looks: its serial number tells.
        *b.there.lock().unwrap() = vec![port("/dev/robot1", "A")];
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        *b.there.lock().unwrap() = vec![port("/dev/robot1", "Z")];
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        assert_eq!(b.probes.lock().unwrap().len(), 3);
        // Without `probe` nothing is opened.
        *b.there.lock().unwrap() = vec![port("/dev/robot2", "Q")];
        assert_eq!(known(&b.d.ports(false), "/dev/robot2"), Known::Waiting);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(b.probes.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_reserved_port_is_not_probed_and_a_connect_waits_for_the_probe() {
        let b = bench(150);
        *b.there.lock().unwrap() = vec![port("/dev/robot1", "A"), port("/dev/robot2", "B")];
        // Reserved before anybody looked: only the other one is asked.
        b.d.reserve(Some("/dev/robot1"));
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        assert_eq!(*b.probes.lock().unwrap(), vec!["r /dev/robot2".to_string()]);
        assert_eq!(known(&b.d.ports(true), "/dev/robot1"), Known::Waiting);
        // The connect says what it found.
        b.d.identified("/dev/robot1", "r", json!({}));
        b.d.reserve(None);
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        assert_eq!(b.probes.lock().unwrap().len(), 1);
        // A probe under way is waited for.
        *b.there.lock().unwrap() = vec![port("/dev/robot3", "C")];
        b.d.ports(true);
        std::thread::sleep(Duration::from_millis(30));
        let t = Instant::now();
        let view = b.d.reserve(Some("/dev/robot3")).unwrap();
        assert!(matches!(view.known, Known::Robot("r", _)), "{:?}", view.known);
        assert!(t.elapsed() >= Duration::from_millis(60));
    }

    #[test]
    fn a_busy_port_is_tried_again_later() {
        let b = bench(1);
        *b.there.lock().unwrap() = vec![port("/dev/busy1", "A")];
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        assert_eq!(known(&b.d.ports(true), "/dev/busy1"), Known::Busy);
        assert!(b.d.settle(Duration::from_secs(5)));
        assert_eq!(b.probes.lock().unwrap().len(), 1, "not again at once");
        // The other program lets go; after a while the port is asked again.
        b.held.store(0, Ordering::SeqCst);
        b.d.inner.state.lock().unwrap().ports[0].at = Instant::now() - BUSY_RETRY;
        b.d.ports(true);
        assert!(b.d.settle(Duration::from_secs(5)));
        assert!(matches!(known(&b.d.ports(true), "/dev/busy1"), Known::Robot("r", _)));
    }
}
