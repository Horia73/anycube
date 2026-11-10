//! A cube of any size as a picture of its stickers.
//!
//! This is the common ground for every size: scrambles are applied to it and
//! solutions are checked on it. Moves are built from geometry, so nothing
//! here is specific to one size. The solvers keep their own compact models.
//!
//! Facelets are in URFDLB order, each face row by row, as in Kociemba's
//! notation. Move `(face + 6 * (layers - 1)) * 3 + power` turns the outer
//! `layers` layers of `face`; for a 4x4x4 this is the numbering of `cube.rs`.

/// Outward normal, column direction and row direction of each face (URFDLB),
/// in a frame with x to the right, y up and z to the front.
const FACES: [([i32; 3], [i32; 3], [i32; 3]); 6] = [
    ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
    ([1, 0, 0], [0, 0, -1], [0, -1, 0]),
    ([0, 0, 1], [1, 0, 0], [0, -1, 0]),
    ([0, -1, 0], [1, 0, 0], [0, 0, -1]),
    ([-1, 0, 0], [0, 0, 1], [0, -1, 0]),
    ([0, 0, -1], [-1, 0, 0], [0, -1, 0]),
];

fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A quarter turn clockwise, seen from outside along `axis`.
fn turn(axis: [i32; 3], v: [i32; 3]) -> [i32; 3] {
    let (c, d) = (cross(axis, v), dot(axis, v));
    [axis[0] * d - c[0], axis[1] * d - c[1], axis[2] * d - c[2]]
}

/// Sticker geometry and move permutations for one cube size.
pub struct Geometry {
    pub n: usize,
    /// `moves[m][j]` is the facelet whose sticker move `m` brings to facelet `j`.
    moves: Vec<Vec<u32>>,
}

impl Geometry {
    pub fn new(n: usize) -> Geometry {
        assert!(n >= 2);
        let nn = n as i32;
        // Position (doubled, so that it is integral) and normal of every facelet.
        let place = |i: usize| -> ([i32; 3], [i32; 3]) {
            let (f, r, c) = (i / (n * n), (i / n % n) as i32, (i % n) as i32);
            let (normal, right, down) = FACES[f];
            let (a, b) = (2 * c - (nn - 1), 2 * r - (nn - 1));
            let mut p = [0; 3];
            for k in 0..3 {
                p[k] = normal[k] * nn + right[k] * a + down[k] * b;
            }
            (p, normal)
        };
        let index = |p: [i32; 3], normal: [i32; 3]| -> usize {
            let f = FACES.iter().position(|x| x.0 == normal).unwrap();
            let (_, right, down) = FACES[f];
            let c = (dot(p, right) + nn - 1) / 2;
            let r = (dot(p, down) + nn - 1) / 2;
            f * n * n + r as usize * n + c as usize
        };
        let total = 6 * n * n;
        let mut moves = Vec::new();
        for layers in 1..=n / 2 {
            for f in 0..6 {
                let axis = FACES[f].0;
                // The quarter turn, then its square and cube.
                let mut quarter: Vec<u32> = (0..total as u32).collect();
                for i in 0..total {
                    let (p, normal) = place(i);
                    // A sticker is in the outer `layers` layers if its cubie is.
                    let depth = dot(p, axis).min(nn - 1);
                    if depth > nn - 1 - 2 * layers as i32 {
                        quarter[index(turn(axis, p), turn(axis, normal))] = i as u32;
                    }
                }
                let half: Vec<u32> = (0..total).map(|j| quarter[quarter[j] as usize]).collect();
                let back: Vec<u32> = (0..total).map(|j| quarter[half[j] as usize]).collect();
                moves.push(quarter);
                moves.push(half);
                moves.push(back);
            }
        }
        Geometry { n, moves }
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    pub fn solved(&self) -> Vec<u8> {
        (0..6 * self.n * self.n).map(|i| (i / (self.n * self.n)) as u8).collect()
    }

    pub fn apply(&self, facelets: &mut Vec<u8>, m: usize) {
        let src = &self.moves[m];
        let old = facelets.clone();
        for (j, f) in facelets.iter_mut().enumerate() {
            *f = old[src[j] as usize];
        }
    }

    pub fn apply_seq(&self, facelets: &mut Vec<u8>, seq: &[usize]) {
        for &m in seq {
            self.apply(facelets, m);
        }
    }

    /// Every face shows a single colour.
    pub fn is_solved(&self, facelets: &[u8]) -> bool {
        facelets.chunks(self.n * self.n).all(|face| face.iter().all(|&c| c == face[0]))
    }

    /// `U`, `Uw` (two layers), `3Uw` (three layers), with `2` and `'` suffixes.
    pub fn move_name(&self, m: usize) -> String {
        let (f, layers, power) = (m / 3 % 6, m / 18 + 1, m % 3);
        let face = ['U', 'R', 'F', 'D', 'L', 'B'][f];
        let body = match layers {
            1 => face.to_string(),
            2 => format!("{}w", face),
            k => format!("{}{}w", k, face),
        };
        body + ["", "2", "'"][power]
    }

    pub fn parse_moves(&self, s: &str) -> Result<Vec<usize>, String> {
        let mut out = Vec::new();
        for tok in s.split_whitespace() {
            let bad = || format!("bad move '{}' for a {}x{}x{}", tok, self.n, self.n, self.n);
            let digits = tok.bytes().take_while(u8::is_ascii_digit).count();
            let mut layers: usize = if digits > 0 { tok[..digits].parse().map_err(|_| bad())? } else { 1 };
            let rest = &tok.as_bytes()[digits..];
            // A lower-case face letter is a two-layer turn, as in `Uw`.
            let first = *rest.first().ok_or_else(bad)?;
            if first.is_ascii_lowercase() {
                layers = layers.max(2);
            }
            let face = b"URFDLB".iter().position(|&x| x == first.to_ascii_uppercase()).ok_or_else(bad)?;
            let mut power = 0;
            for &ch in &rest[1..] {
                match ch {
                    b'w' => layers = layers.max(2),
                    b'2' => power = 1,
                    b'\'' => power = 2,
                    _ => return Err(bad()),
                }
            }
            if layers == 0 || layers > self.n / 2 {
                return Err(bad());
            }
            out.push((face + 6 * (layers - 1)) * 3 + power);
        }
        Ok(out)
    }

    pub fn to_string(&self, facelets: &[u8]) -> String {
        facelets.iter().map(|&c| b"URFDLB"[c as usize] as char).collect()
    }

    pub fn from_string(&self, s: &str) -> Result<Vec<u8>, String> {
        let b = s.trim().as_bytes();
        if b.len() != 6 * self.n * self.n {
            return Err(format!("expected {} facelets, got {}", 6 * self.n * self.n, b.len()));
        }
        b.iter()
            .map(|c| b"URFDLB".iter().position(|x| x == c).map(|p| p as u8).ok_or_else(|| "bad facelet".to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::Cube;

    #[test]
    fn agrees_with_the_4x4x4_model() {
        let g = Geometry::new(4);
        assert_eq!(g.move_count(), 36);
        let mut seed = 7u64;
        let mut facelets = g.solved();
        let mut cube = Cube::solved();
        for _ in 0..200 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let m = (seed >> 33) as usize % 36;
            g.apply(&mut facelets, m);
            cube.apply(m as u8);
            assert_eq!(g.to_string(&facelets), cube.to_facelets());
        }
    }

    #[test]
    fn moves_have_order_four_on_every_size() {
        for n in 2..=7 {
            let g = Geometry::new(n);
            assert_eq!(g.move_count(), 18 * (n / 2));
            for m in (0..g.move_count()).step_by(3) {
                let mut f = g.solved();
                // Mark every sticker so that the test sees more than face colours.
                for (i, x) in f.iter_mut().enumerate() {
                    *x = (i % 251) as u8;
                }
                let start = f.clone();
                g.apply(&mut f, m);
                assert_ne!(f, start);
                g.apply(&mut f, m + 1);
                g.apply(&mut f, m);
                assert_eq!(f, start, "size {} move {}", n, g.move_name(m));
            }
        }
    }

    #[test]
    fn names_round_trip() {
        let g = Geometry::new(7);
        for m in 0..g.move_count() {
            assert_eq!(g.parse_moves(&g.move_name(m)).unwrap(), vec![m]);
        }
        assert!(Geometry::new(5).parse_moves("3Uw").is_err());
    }
}
