//! What a MultiCuber-style robot's commands do to a real cube, on the
//! sticker model: used by the simulated robot and by the tests that check
//! that every plan leaves the cube solved.
//!
//! Positions are the robot's own (the camera's U, the flipper's F). At lift
//! level k the rotator turns the k top layers; at level n the whole cube
//! (y). With the lift at 0 the flipper tips the cube (UP: the front goes to
//! the top, x; DOWN: x'); higher up it moves alone. This is the model of
//! MultiCuber's tests/test_robot.py.

use crate::cuben::Geometry;
use crate::robot::multicuber::planner::Lift;

pub struct PhysicalCube {
    pub g: Geometry,
    pub facelets: Vec<u8>,
    pub lift: Lift,
    pub flip_up: bool,
    /// Steps that the planner never makes and that the robot must not do.
    pub violations: Vec<String>,
}

impl PhysicalCube {
    pub fn new(n: usize) -> PhysicalCube {
        let g = Geometry::new(n);
        let facelets = g.solved();
        PhysicalCube { g, facelets, lift: Lift::Level(0), flip_up: true, violations: Vec::new() }
    }

    pub fn n(&self) -> usize {
        self.g.n
    }

    fn whole(&mut self, axis: usize, turns: i8) {
        let k = match turns {
            1 => 1,
            2 => 2,
            _ => 3,
        };
        self.facelets = self.g.turned(&self.facelets, &vec![axis; k]);
    }

    fn layers(&mut self, face: usize, layers: usize, turns: i8) {
        let power = match turns {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        self.g.apply(&mut self.facelets, (face + 6 * (layers - 1)) * 3 + power);
    }

    /// One command line, as the firmware takes it. Only motion changes anything.
    pub fn apply_line(&mut self, line: &str) {
        let w: Vec<String> = line.split_whitespace().map(|s| s.to_ascii_uppercase()).collect();
        let n = self.n() as u8;
        match w.first().map(String::as_str) {
            Some("LIFT") => {
                let to = match w.get(1).map(String::as_str) {
                    Some("DOWN") => Lift::Level(0),
                    Some("UP") => Lift::Level(n),
                    Some(a) => match Lift::parse(a) {
                        Some(l) => l,
                        None => return,
                    },
                    None => return,
                };
                // The planner only reaches the scanning height from 0 and leaves it to 0.
                let between = |a: Lift, b: Lift| a == Lift::Scan && !matches!(b, Lift::Level(0) | Lift::Scan);
                if between(self.lift, to) || between(to, self.lift) {
                    self.violations.push(format!("LIFT between SCAN and a level: {:?} -> {:?}", self.lift, to));
                }
                self.lift = to;
            }
            Some("FLIP") => {
                let up = w.get(1).map(String::as_str) == Some("UP");
                if up != self.flip_up && self.lift == Lift::Level(0) {
                    self.whole(0, if up { 1 } else { -1 });
                }
                self.flip_up = up;
            }
            Some("ROT") => {
                let t: i8 = w.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                match self.lift {
                    Lift::Scan | Lift::Level(0) => self.violations.push(format!("ROT with the lift at {:?}", self.lift)),
                    Lift::Level(k) if k >= n => self.whole(1, t),
                    Lift::Level(k) => {
                        let (k, nn) = (k as usize, n as usize);
                        if k <= nn / 2 {
                            self.layers(0, k, t);
                        } else {
                            // The top k layers turning = the whole cube turning and the bottom n-k turning back (as D).
                            self.whole(1, t);
                            self.layers(3, nn - k, t);
                        }
                    }
                }
            }
            Some("ZERO") => {
                self.lift = Lift::Level(0);
                self.flip_up = true;
            }
            _ => {}
        }
    }

    pub fn is_solved(&self) -> bool {
        self.g.is_solved(&self.facelets)
    }

    /// The facelets as letters, the way /api/check takes them.
    pub fn facelet_string(&self) -> String {
        self.g.to_string(&self.facelets)
    }

    /// Scrambles with random face and inner-layer moves.
    pub fn scramble(&mut self, seed: u64, count: usize) {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        for _ in 0..count {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let m = (s % self.g.move_count() as u64) as usize;
            self.g.apply(&mut self.facelets, m);
        }
    }
}
