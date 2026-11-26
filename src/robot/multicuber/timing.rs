//! How long MultiCuber's commands take.
//!
//! Every motion is one motor going through a linear ramp to 600 rpm (2000
//! full steps/s) and back down, so its time follows from its length in
//! full steps and its acceleration. The lengths come from the firmware's
//! calibration table of the cube size (`CAL`), the accelerations from the
//! MEDIUM preset. On the robot's measurements of 26.11.2026 (3x3x3,
//! firmware 2.2, 1/8 microstepping, `timing-2026-11-26.json`) this
//! physics is within ~3 % of every pair, a little long because the firmware
//! starts each ramp at its first step's speed: one factor per kind of
//! motion, fitted to those measurements, takes that out. So the levels of
//! a 4x4x4 or 5x5x5, and lift pairs nobody measured, get times that are
//! interpolated through the measured ones.
//!
//! On top of that, the times of the commands the robot really runs are
//! kept (an average per command and size, which wins over the model once
//! there is one) and saved with anycube's settings.

use super::planner::{Cmd, Lift, Op};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// The measured medians, as the timing run wrote them.
pub const MEASURED_JSON: &str = include_str!("timing-2026-11-26.json");

/// Top speed, full steps per second (600 rpm of 200-step motors).
const VMAX: f64 = 2000.0;
/// MEDIUM preset accelerations (full steps/s²), firmware/config.h.
const ACCEL_LIFT: f64 = 8000.0;
const ACCEL_ROT: f64 = 8000.0;
const ACCEL_FLIP: f64 = 8000.0;
const ACCEL_FAST: f64 = 16000.0;
/// The soft enable ramp (SOFT_ENABLE_MS) plus the driver wake-up.
const ENABLE_MS: f64 = 251.0;

/// The firmware's calibration of one cube size, in full steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Cal {
    pub n: usize,
    /// Heights of levels 0..=n (`lift[0]` = 0).
    pub lift: Vec<i32>,
    pub scan: i32,
    pub flip: i32,
    pub rotq: i32,
    pub roto: i32,
}

impl Cal {
    /// The built-in table of firmware/config.h for a size (3, 4, 5), or a
    /// guess in its spirit for another.
    pub fn default_for(n: usize) -> Cal {
        let (lift, roto): (Vec<i32>, i32) = match n {
            3 => (vec![0, 370, 440, 510], 95),
            4 => (vec![0, 330, 390, 450, 510], 55),
            5 => (vec![0, 325, 372, 419, 466, 513], 45),
            _ => {
                let mut v = vec![0];
                for k in 1..=n {
                    v.push(320 + (190 * k as i32) / n as i32);
                }
                (v, 45)
            }
        };
        Cal { n, lift, scan: 200, flip: 50, rotq: 175, roto }
    }

    /// The line `CAL` prints: `# n=3 lift=370,440,510 scan=200 flip=50 rotq=175 roto=95 half=1`.
    pub fn parse(line: &str) -> Option<Cal> {
        let mut fields = BTreeMap::new();
        for kv in line.trim_start_matches('#').split_whitespace() {
            if let Some((k, v)) = kv.split_once('=') {
                fields.insert(k.to_ascii_lowercase(), v.to_string());
            }
        }
        let n: usize = fields.get("n")?.parse().ok()?;
        let mut lift = vec![0];
        for v in fields.get("lift")?.split(',') {
            lift.push(v.parse().ok()?);
        }
        if lift.len() != n + 1 {
            return None;
        }
        Some(Cal {
            n,
            lift,
            scan: fields.get("scan")?.parse().ok()?,
            flip: fields.get("flip")?.parse().ok()?,
            rotq: fields.get("rotq")?.parse().ok()?,
            roto: fields.get("roto")?.parse().ok()?,
        })
    }

    pub fn height(&self, l: Lift) -> i32 {
        match l {
            Lift::Scan => self.scan,
            Lift::Level(k) => self.lift.get(k as usize).copied().unwrap_or(0),
        }
    }

    /// The level at a height the firmware reports (`STATE` lift=...), if it is one.
    pub fn level_at(&self, height: i32) -> Option<Lift> {
        if height == 0 {
            return Some(Lift::Level(0));
        }
        if let Some(k) = self.lift.iter().position(|&h| h == height) {
            return Some(Lift::Level(k as u8));
        }
        (height == self.scan).then_some(Lift::Scan)
    }
}

/// Seconds for `d` full steps through a linear ramp of acceleration `a` to VMAX and back.
fn ramp_ms(d: f64, a: f64) -> f64 {
    if d <= 0.0 {
        return 0.0;
    }
    let s = if d >= VMAX * VMAX / a { d / VMAX + VMAX / a } else { 2.0 * (d / a).sqrt() };
    1000.0 * s
}

/// The kinds of motion, each with its own correction factor.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Kind {
    LiftUp,
    LiftDown,
    FlipCarry,
    FlipEmpty,
    Rot,
    Other,
}

const KINDS: [(Kind, &str); 5] = [
    (Kind::LiftUp, "lift_up"),
    (Kind::LiftDown, "lift_down"),
    (Kind::FlipCarry, "flip_carry"),
    (Kind::FlipEmpty, "flip_empty"),
    (Kind::Rot, "rot"),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stat {
    pub ms: f64,
    pub n: u32,
}

/// One cube size's command times.
#[derive(Clone, Debug)]
pub struct TimeModel {
    pub cal: Cal,
    /// Physics -> measured, per kind of motion.
    factor: BTreeMap<Kind, f64>,
    /// Medians of the timing run, by command key (3x3x3 only).
    defaults: BTreeMap<String, f64>,
    /// Times measured on this robot while it worked, by command key.
    pub learned: BTreeMap<String, Stat>,
    /// Added to every command: what is left of the serial round trip with
    /// the commands sent ahead.
    pub overhead_ms: f64,
    /// The round trip of one command when it is not sent ahead.
    pub ping_ms: f64,
    // Lookup tables for `op_ms`, rebuilt after every change.
    lift_ms: Vec<f64>,
    flip_ms: [f64; 3],
    rot_ms: [f64; 2],
}

/// The key of a lift position in command keys: `0`..`n`, `SCAN`.
fn lift_key(l: Lift) -> String {
    l.arg()
}

/// The command key an op's time is kept under.
pub fn op_key(op: &Op) -> String {
    match op.cmd {
        Cmd::Lift(to) => format!("LIFT {}>{}", lift_key(op.lift_before), lift_key(to)),
        Cmd::Flip { normal, .. } => {
            if op.carries() {
                "FLIP carry".into()
            } else if normal {
                "FLIP empty-normal".into()
            } else {
                "FLIP empty".into()
            }
        }
        Cmd::Rot(t) => if t == 2 { "ROT 2" } else { "ROT 1" }.into(),
        Cmd::Enable => "ENABLE".into(),
        Cmd::State => "STATE".into(),
        Cmd::Disable => "DISABLE".into(),
    }
}

impl TimeModel {
    /// The model for a size: physics on its calibration, the factors from
    /// the measured medians.
    pub fn new(cal: Cal) -> TimeModel {
        let measured: Value = serde_json::from_str(MEASURED_JSON).expect("the built-in timing file is JSON");
        let med = |k: &str| measured[k]["median_ms"].as_f64();
        let mut defaults = BTreeMap::new();
        // The run's names -> command keys.
        if let Some(obj) = measured.as_object() {
            for (name, v) in obj {
                let Some(ms) = v["median_ms"].as_f64() else { continue };
                if let Some(pair) = name.strip_prefix("lift ") {
                    // "lift SCAN->0 x" was a repeat under other conditions; the plain one stands.
                    if let Some((a, b)) = pair.split_once("->").filter(|(_, b)| !b.contains(' ')) {
                        defaults.insert(format!("LIFT {}>{}", a, b), ms);
                    }
                }
            }
        }
        let avg = |names: &[&str]| {
            let v: Vec<f64> = names.iter().filter_map(|n| med(n)).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        if let Some(ms) = avg(&["flip UP carry", "flip DOWN carry"]) {
            defaults.insert("FLIP carry".into(), ms);
        }
        if let Some(ms) = avg(&["flip DOWN empty", "flip UP empty"]) {
            defaults.insert("FLIP empty".into(), ms);
        }
        if let Some(ms) = avg(&["rot 1 (lift1)", "rot -1 (lift1)", "rot 1 (lift3)", "rot -1 (lift3)"]) {
            defaults.insert("ROT 1".into(), ms);
        }
        if let Some(ms) = avg(&["rot 2 (lift1)", "rot 2 (lift3)"]) {
            defaults.insert("ROT 2".into(), ms);
        }
        let ping = med("ping").unwrap_or(12.0);

        // Fit one factor per kind on the 3x3x3 table the measurements were made with.
        let base = Cal::default_for(3);
        let mut sums: BTreeMap<Kind, (f64, f64)> = BTreeMap::new();
        for (key, &ms) in &defaults {
            if let Some((kind, phys)) = physics(&base, key) {
                let e = sums.entry(kind).or_insert((0.0, 0.0));
                e.0 += ms * phys;
                e.1 += phys * phys;
            }
        }
        let factor = KINDS.iter().map(|&(k, _)| (k, sums.get(&k).map_or(1.0, |&(a, b)| if b > 0.0 { a / b } else { 1.0 }))).collect();
        // The measurements belong to the 3x3x3 calibration: another table
        // keeps those whose motion is the same length there, and the fit.
        if cal != base {
            defaults.retain(|k, _| match (physics(&base, k), physics(&cal, k)) {
                (Some((_, a)), Some((_, b))) => (a - b).abs() < 1e-9,
                _ => false,
            });
        }
        let mut m = TimeModel {
            cal,
            factor,
            defaults,
            learned: BTreeMap::new(),
            overhead_ms: 1.0,
            ping_ms: ping,
            lift_ms: Vec::new(),
            flip_ms: [0.0; 3],
            rot_ms: [0.0; 2],
        };
        m.rebuild();
        m
    }

    pub fn for_size(n: usize) -> TimeModel {
        TimeModel::new(Cal::default_for(n))
    }

    /// The model's time for a command key, before what was learned.
    pub fn prior_ms(&self, key: &str) -> f64 {
        if let Some(&ms) = self.defaults.get(key) {
            return ms;
        }
        match physics(&self.cal, key) {
            Some((kind, phys)) => phys * self.factor.get(&kind).copied().unwrap_or(1.0),
            None => match key {
                "ENABLE" => ENABLE_MS,
                "FLIP empty-normal" => self.prior_ms("FLIP carry"),
                _ => 0.0,
            },
        }
    }

    /// The best estimate for a command key.
    pub fn key_ms(&self, key: &str) -> f64 {
        match self.learned.get(key) {
            Some(s) if s.n > 0 => s.ms,
            _ => self.prior_ms(key),
        }
    }

    fn lift_index(&self, l: Lift) -> usize {
        match l {
            Lift::Level(k) => (k as usize).min(self.cal.n),
            Lift::Scan => self.cal.n + 1,
        }
    }

    fn rebuild(&mut self) {
        let m = self.cal.n + 2;
        let all: Vec<Lift> = (0..=self.cal.n as u8).map(Lift::Level).chain([Lift::Scan]).collect();
        let mut table = vec![0.0; m * m];
        for &a in &all {
            for &b in &all {
                if a != b {
                    let key = format!("LIFT {}>{}", lift_key(a), lift_key(b));
                    table[self.lift_index(a) * m + self.lift_index(b)] = self.key_ms(&key);
                }
            }
        }
        self.lift_ms = table;
        self.flip_ms = [self.key_ms("FLIP carry"), self.key_ms("FLIP empty"), self.key_ms("FLIP empty-normal")];
        self.rot_ms = [self.key_ms("ROT 1"), self.key_ms("ROT 2")];
    }

    /// The time of one command (fast: this runs for every command of every candidate).
    #[inline]
    pub fn op_ms(&self, op: &Op) -> f64 {
        self.overhead_ms
            + match op.cmd {
                Cmd::Lift(to) => self.lift_ms[self.lift_index(op.lift_before) * (self.cal.n + 2) + self.lift_index(to)],
                Cmd::Flip { normal, .. } => {
                    if op.lift_before == Lift::Level(0) {
                        self.flip_ms[0]
                    } else if normal {
                        self.flip_ms[2]
                    } else {
                        self.flip_ms[1]
                    }
                }
                Cmd::Rot(t) => self.rot_ms[(t == 2) as usize],
                Cmd::Enable => self.key_ms("ENABLE"),
                Cmd::State | Cmd::Disable => 0.0,
            }
    }

    /// A command really took `ms`. Returns false when the time was set
    /// aside as not belonging to the command (a stall, a delay of the link).
    pub fn record(&mut self, op: &Op, ms: f64) -> bool {
        let key = op_key(op);
        if matches!(op.cmd, Cmd::State | Cmd::Disable) {
            return false;
        }
        let expect = self.key_ms(&key);
        if !(ms.is_finite() && expect > 0.0 && ms > 0.5 * expect && ms < 2.0 * expect) {
            return false;
        }
        let s = self.learned.entry(key).or_insert(Stat { ms: 0.0, n: 0 });
        s.n = s.n.saturating_add(1);
        // The mean of the first few, then a moving average that follows slow changes.
        let w = (1.0 / s.n as f64).max(0.2);
        s.ms += w * (ms - s.ms);
        self.rebuild();
        true
    }

    pub fn to_json(&self) -> Value {
        let ops: Map<String, Value> =
            self.learned.iter().map(|(k, s)| (k.clone(), json!({"ms": (s.ms * 10.0).round() / 10.0, "n": s.n}))).collect();
        json!({"cal": {"lift": self.cal.lift, "scan": self.cal.scan, "flip": self.cal.flip, "rotq": self.cal.rotq, "roto": self.cal.roto}, "ops": ops})
    }

    /// Takes what was learned from a saved model of the same calibration.
    pub fn absorb(&mut self, saved: &Value) {
        let cal = &saved["cal"];
        let same = cal["lift"].as_array().map(|v| v.iter().filter_map(Value::as_i64).map(|x| x as i32).collect::<Vec<_>>())
            == Some(self.cal.lift.clone())
            && cal["roto"].as_i64() == Some(self.cal.roto as i64)
            && cal["scan"].as_i64() == Some(self.cal.scan as i64);
        if !same {
            return;
        }
        if let Some(ops) = saved["ops"].as_object() {
            for (k, v) in ops {
                if let (Some(ms), Some(n)) = (v["ms"].as_f64(), v["n"].as_u64()) {
                    if ms > 0.0 && n > 0 {
                        self.learned.insert(k.clone(), Stat { ms, n: n.min(1000) as u32 });
                    }
                }
            }
        }
        self.rebuild();
    }
}

/// Physics time of a command key on a calibration, and its kind.
fn physics(cal: &Cal, key: &str) -> Option<(Kind, f64)> {
    if let Some(pair) = key.strip_prefix("LIFT ") {
        let (a, b) = pair.split_once('>')?;
        let (a, b) = (Lift::parse(a)?, Lift::parse(b)?);
        let d = (cal.height(a) - cal.height(b)).abs() as f64;
        // Going down to 0 uses the fast acceleration; everything else the preset.
        return Some(if b == Lift::Level(0) { (Kind::LiftDown, ramp_ms(d, ACCEL_FAST)) } else { (Kind::LiftUp, ramp_ms(d, ACCEL_LIFT)) });
    }
    let (q, o) = (cal.rotq as f64, cal.roto as f64);
    match key {
        "FLIP carry" => Some((Kind::FlipCarry, ramp_ms(cal.flip as f64, ACCEL_FLIP))),
        "FLIP empty" => Some((Kind::FlipEmpty, ramp_ms(cal.flip as f64, ACCEL_FAST))),
        // Overshoot and back: two moves.
        "ROT 1" => Some((Kind::Rot, ramp_ms(q + o, ACCEL_ROT) + ramp_ms(o, ACCEL_ROT))),
        "ROT 2" => Some((Kind::Rot, ramp_ms(2.0 * q + o, ACCEL_ROT) + ramp_ms(o, ACCEL_ROT))),
        _ => None,
    }
}

/// The saved models of every size, in one file.
pub fn load_saved(path: &Path) -> Value {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

pub fn save(path: &Path, model: &TimeModel) -> std::io::Result<()> {
    let mut all = load_saved(path);
    all["version"] = json!(1);
    all["driver"] = json!("multicuber");
    if !all["sizes"].is_object() {
        all["sizes"] = json!({});
    }
    all["sizes"][model.cal.n.to_string()] = model.to_json();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(&all).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(m: &TimeModel, a: Lift, b: Lift) -> f64 {
        m.key_ms(&format!("LIFT {}>{}", a.arg(), b.arg()))
    }

    #[test]
    fn physics_fits_the_measured_pairs() {
        let base = Cal::default_for(3);
        let m = TimeModel::for_size(3);
        for (k, &ms) in &m.defaults {
            if let Some((kind, phys)) = physics(&base, k) {
                let model = phys * m.factor[&kind];
                assert!((model - ms).abs() / ms < 0.04, "{}: model {:.0} measured {:.0}", k, model, ms);
            }
        }
        for f in m.factor.values() {
            assert!((0.9..1.05).contains(f), "factor {}", f);
        }
    }

    #[test]
    fn unknown_pairs_are_interpolated() {
        let m = TimeModel::for_size(3);
        // 1->3 was not measured; it is between 1->2 and 0->2 in length.
        let t13 = key(&m, Lift::Level(1), Lift::Level(3));
        assert!(t13 > key(&m, Lift::Level(1), Lift::Level(2)) && t13 < key(&m, Lift::Level(0), Lift::Level(2)));
        // Measured pairs are taken as they are.
        assert_eq!(key(&m, Lift::Level(0), Lift::Level(1)), 425.5);
        let m5 = TimeModel::for_size(5);
        let t = key(&m5, Lift::Level(0), Lift::Level(5));
        assert!((480.0..520.0).contains(&t), "{}", t);
        // A smaller overshoot makes the 5x5x5 quarter turn quicker.
        assert!(m5.key_ms("ROT 1") < m.key_ms("ROT 1"));
    }

    #[test]
    fn learning_follows_the_robot_and_is_saved() {
        let mut m = TimeModel::for_size(3);
        let op = Op { cmd: Cmd::Rot(1), remap: super::super::planner::Remap::None, photo: None, mv: None, lift_before: Lift::Level(1) };
        let before = m.op_ms(&op);
        for _ in 0..20 {
            assert!(m.record(&op, 600.0));
        }
        assert!(!m.record(&op, 5000.0), "a stall is not learned");
        assert!((m.op_ms(&op) - m.overhead_ms - 600.0).abs() < 1.0 && before < 590.0);
        let path = std::env::temp_dir().join(format!("anycube-timing-test-{}.json", std::process::id()));
        save(&path, &m).unwrap();
        let mut again = TimeModel::for_size(3);
        again.absorb(&load_saved(&path)["sizes"]["3"]);
        assert!((again.op_ms(&op) - m.op_ms(&op)).abs() < 0.2);
        // Another calibration does not take it.
        let mut other = TimeModel::for_size(4);
        other.absorb(&load_saved(&path)["sizes"]["3"]);
        assert!(other.learned.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn cal_line_parses() {
        let c = Cal::parse("# n=3 lift=370,440,510 scan=200 flip=50 rotq=175 roto=95 half=1").unwrap();
        assert_eq!(c, Cal::default_for(3));
        assert_eq!(c.level_at(440), Some(Lift::Level(2)));
        assert_eq!(c.level_at(200), Some(Lift::Scan));
        assert_eq!(c.level_at(123), None);
    }
}
