//! Full 4x4x4 cube model: 24 centres, 24 wing edges, 8 corners.
//!
//! Piece numbering and move definitions follow Chen Shuang's TPR solver
//! (https://github.com/cs0x7f/TPR-4x4x4-Solver, MIT), so states and facelet
//! strings are interchangeable with it.
//!
//! Moves are numbered `face * 3 + power`, with faces
//! `U R F D L B Uw Rw Fw Dw Lw Bw` and power 0 = clockwise, 1 = half turn,
//! 2 = counter-clockwise.

use std::sync::OnceLock;

pub const N_MOVES: usize = 36;

pub const MOVE_NAMES: [&str; N_MOVES] = [
    "U", "U2", "U'", "R", "R2", "R'", "F", "F2", "F'", "D", "D2", "D'", "L", "L2", "L'", "B", "B2",
    "B'", "Uw", "Uw2", "Uw'", "Rw", "Rw2", "Rw'", "Fw", "Fw2", "Fw'", "Dw", "Dw2", "Dw'", "Lw",
    "Lw2", "Lw'", "Bw", "Bw2", "Bw'",
];

/// Colours: U=0 R=1 F=2 D=3 L=4 B=5.
pub const SOLVED_CT: [u8; 24] = [
    0, 0, 0, 0, 3, 3, 3, 3, 2, 2, 2, 2, 5, 5, 5, 5, 1, 1, 1, 1, 4, 4, 4, 4,
];
/// First centre position of the face whose home colour is `c`.
const HOME: [usize; 6] = [0, 16, 8, 4, 20, 12];

const CT_CYC: [&[[u8; 4]]; 12] = [
    &[[0, 1, 2, 3]],
    &[[16, 17, 18, 19]],
    &[[8, 9, 10, 11]],
    &[[4, 5, 6, 7]],
    &[[20, 21, 22, 23]],
    &[[12, 13, 14, 15]],
    &[[0, 1, 2, 3], [8, 20, 12, 16], [9, 21, 13, 17]],
    &[[16, 17, 18, 19], [1, 15, 5, 9], [2, 12, 6, 10]],
    &[[8, 9, 10, 11], [2, 19, 4, 21], [3, 16, 5, 22]],
    &[[4, 5, 6, 7], [10, 18, 14, 22], [11, 19, 15, 23]],
    &[[20, 21, 22, 23], [0, 8, 4, 14], [3, 11, 7, 13]],
    &[[12, 13, 14, 15], [1, 20, 7, 18], [0, 23, 6, 17]],
];

const EP_CYC: [&[[u8; 4]]; 12] = [
    &[[0, 1, 2, 3], [12, 13, 14, 15]],
    &[[11, 15, 10, 19], [23, 3, 22, 7]],
    &[[0, 11, 6, 8], [12, 23, 18, 20]],
    &[[4, 5, 6, 7], [16, 17, 18, 19]],
    &[[1, 20, 5, 21], [13, 8, 17, 9]],
    &[[2, 9, 4, 10], [14, 21, 16, 22]],
    &[[0, 1, 2, 3], [12, 13, 14, 15], [9, 22, 11, 20]],
    &[[11, 15, 10, 19], [23, 3, 22, 7], [2, 16, 6, 12]],
    &[[0, 11, 6, 8], [12, 23, 18, 20], [3, 19, 5, 13]],
    &[[4, 5, 6, 7], [16, 17, 18, 19], [8, 23, 10, 21]],
    &[[1, 20, 5, 21], [13, 8, 17, 9], [14, 0, 18, 4]],
    &[[2, 9, 4, 10], [14, 21, 16, 22], [7, 15, 1, 17]],
];

/// (corner permutation index, twist index) of the six clockwise face turns.
const CORNER_MOVES: [(u32, u32); 6] =
    [(15120, 0), (21021, 1494), (8064, 1236), (9, 0), (1230, 412), (224, 137)];

#[inline]
pub fn inv_move(m: u8) -> u8 {
    m / 3 * 3 + (2 - m % 3)
}

pub fn inv_seq(seq: &[u8]) -> Vec<u8> {
    seq.iter().rev().map(|&m| inv_move(m)).collect()
}

/// True when move `cur` must not follow `prev` in a canonical sequence:
/// same face, or same axis in decreasing order.
#[inline]
pub fn skip_move(prev: u8, cur: u8) -> bool {
    let (a, b) = (prev / 3, cur / 3);
    a == b || (a % 3 == b % 3 && prev > cur)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Cube {
    pub ct: [u8; 24],
    pub ep: [u8; 24],
    pub cp: [u8; 8],
    pub co: [u8; 8],
}

pub struct Rot {
    pub seq: Vec<u8>,
    pub state: Cube,
}

pub struct Tables {
    pub ct_src: [[u8; 24]; N_MOVES],
    pub ep_src: [[u8; 24]; N_MOVES],
    pub c_cp: [[u8; 8]; 18],
    pub c_co: [[u8; 8]; 18],
    /// The 24 whole-cube rotations (index 0 is the identity).
    pub rots: Vec<Rot>,
    /// `relabel[r][m]`: the move equal to `rot_r, m, rot_r^-1`.
    pub relabel: Vec<[u8; N_MOVES]>,
    /// Index of the inverse rotation.
    pub rot_inv: Vec<usize>,
}

fn quarter_src(cycles: &[[u8; 4]]) -> [u8; 24] {
    let mut src = [0u8; 24];
    for (i, s) in src.iter_mut().enumerate() {
        *s = i as u8;
    }
    for c in cycles {
        let [a, b, cc, d] = *c;
        src[b as usize] = a;
        src[cc as usize] = b;
        src[d as usize] = cc;
        src[a as usize] = d;
    }
    src
}

fn compose_src(first: &[u8; 24], second: &[u8; 24]) -> [u8; 24] {
    // Apply `first`, then `second`: new[i] = mid[second[i]] = old[first[second[i]]].
    let mut out = [0u8; 24];
    for i in 0..24 {
        out[i] = first[second[i] as usize];
    }
    out
}

fn set8perm(mut idx: u32) -> [u8; 8] {
    const FACT: [u32; 8] = [1, 1, 2, 6, 24, 120, 720, 5040];
    let mut arr = [0u8; 8];
    let mut val: u32 = 0x7654_3210;
    for i in 0..7 {
        let p = FACT[7 - i];
        let v = idx / p;
        idx -= v * p;
        let v = v << 2;
        arr[i] = ((val >> v) & 0xf) as u8;
        let m = (1u32 << v) - 1;
        val = (val & m) + ((val >> 4) & !m);
    }
    arr[7] = (val & 0xf) as u8;
    arr
}

fn set_twist(mut idx: u32) -> [u8; 8] {
    let mut co = [0u8; 8];
    let mut sum = 0u32;
    for i in (0..7).rev() {
        co[i] = (idx % 3) as u8;
        sum += idx % 3;
        idx /= 3;
    }
    co[7] = ((15 - sum) % 3) as u8;
    co
}

fn corner_mult(acp: &[u8; 8], aco: &[u8; 8], bcp: &[u8; 8], bco: &[u8; 8]) -> ([u8; 8], [u8; 8]) {
    let mut cp = [0u8; 8];
    let mut co = [0u8; 8];
    for c in 0..8 {
        cp[c] = acp[bcp[c] as usize];
        co[c] = (aco[bcp[c] as usize] + bco[c]) % 3;
    }
    (cp, co)
}

impl Tables {
    fn build() -> Tables {
        let mut ct_src = [[0u8; 24]; N_MOVES];
        let mut ep_src = [[0u8; 24]; N_MOVES];
        for f in 0..12 {
            let q = quarter_src(CT_CYC[f]);
            let h = compose_src(&q, &q);
            ct_src[f * 3] = q;
            ct_src[f * 3 + 1] = h;
            ct_src[f * 3 + 2] = compose_src(&h, &q);
            let q = quarter_src(EP_CYC[f]);
            let h = compose_src(&q, &q);
            ep_src[f * 3] = q;
            ep_src[f * 3 + 1] = h;
            ep_src[f * 3 + 2] = compose_src(&h, &q);
        }
        let mut c_cp = [[0u8; 8]; 18];
        let mut c_co = [[0u8; 8]; 18];
        for f in 0..6 {
            let (cp, co) = (set8perm(CORNER_MOVES[f].0), set_twist(CORNER_MOVES[f].1));
            c_cp[f * 3] = cp;
            c_co[f * 3] = co;
            for p in 0..2 {
                let (a, b) = corner_mult(&c_cp[f * 3 + p], &c_co[f * 3 + p], &cp, &co);
                c_cp[f * 3 + p + 1] = a;
                c_co[f * 3 + p + 1] = b;
            }
        }
        let mut t = Tables {
            ct_src,
            ep_src,
            c_cp,
            c_co,
            rots: Vec::new(),
            relabel: Vec::new(),
            rot_inv: Vec::new(),
        };
        // Whole-cube rotations: closure of x = Rw Lw' and y = Uw Dw'.
        let gens: [[u8; 2]; 2] = [[21, 32], [18, 29]];
        let mut rots = vec![Rot { seq: vec![], state: Cube::solved() }];
        let mut i = 0;
        while i < rots.len() {
            for g in &gens {
                let mut s = rots[i].state;
                for &m in g {
                    s.apply_with(&t, m);
                }
                if !rots.iter().any(|r| r.state == s) {
                    let mut seq = rots[i].seq.clone();
                    seq.extend_from_slice(g);
                    rots.push(Rot { seq, state: s });
                }
            }
            i += 1;
        }
        assert_eq!(rots.len(), 24);
        let mut relabel = Vec::new();
        let mut rot_inv = Vec::new();
        for r in &rots {
            let inv = inv_seq(&r.seq);
            let mut row = [0u8; N_MOVES];
            for m in 0..N_MOVES as u8 {
                let mut s = r.state;
                s.apply_with(&t, m);
                for &x in &inv {
                    s.apply_with(&t, x);
                }
                let found = (0..N_MOVES as u8).find(|&k| {
                    let mut c = Cube::solved();
                    c.apply_with(&t, k);
                    c == s
                });
                row[m as usize] = found.expect("rotation must map a move to a move");
            }
            relabel.push(row);
            let mut s = Cube::solved();
            for &x in &inv {
                s.apply_with(&t, x);
            }
            rot_inv.push(rots.iter().position(|q| q.state == s).unwrap());
        }
        t.rots = rots;
        t.relabel = relabel;
        t.rot_inv = rot_inv;
        t
    }

    /// Index of the rotation produced by `seq` applied to the solved cube.
    pub fn rot_index(&self, seq: &[u8]) -> usize {
        let mut s = Cube::solved();
        for &m in seq {
            s.apply_with(self, m);
        }
        self.rots.iter().position(|r| r.state == s).expect("not a rotation")
    }
}

pub fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(Tables::build)
}

impl Cube {
    pub fn solved() -> Cube {
        let mut ep = [0u8; 24];
        for (i, e) in ep.iter_mut().enumerate() {
            *e = i as u8;
        }
        Cube { ct: SOLVED_CT, ep, cp: [0, 1, 2, 3, 4, 5, 6, 7], co: [0; 8] }
    }

    #[inline]
    fn apply_with(&mut self, t: &Tables, m: u8) {
        let m = m as usize;
        let (cs, es) = (&t.ct_src[m], &t.ep_src[m]);
        let (ct, ep) = (self.ct, self.ep);
        for i in 0..24 {
            self.ct[i] = ct[cs[i] as usize];
            self.ep[i] = ep[es[i] as usize];
        }
        let k = m % 18;
        let (cp, co) = corner_mult(&self.cp, &self.co, &t.c_cp[k], &t.c_co[k]);
        self.cp = cp;
        self.co = co;
    }

    #[inline]
    pub fn apply(&mut self, m: u8) {
        self.apply_with(tables(), m);
    }

    pub fn apply_seq(&mut self, seq: &[u8]) {
        let t = tables();
        for &m in seq {
            self.apply_with(t, m);
        }
    }

    pub fn from_moves(seq: &[u8]) -> Cube {
        let mut c = Cube::solved();
        c.apply_seq(seq);
        c
    }

    /// The state reached by doing rotation `rot` first, then `self`
    /// (`rot` must be a rotated solved cube).
    pub fn after_rotation(&self, rot: &Cube) -> Cube {
        let mut out = *self;
        for i in 0..24 {
            out.ct[i] = rot.ct[HOME[self.ct[i] as usize]];
            out.ep[i] = rot.ep[self.ep[i] as usize];
        }
        let (cp, co) = corner_mult(&rot.cp, &rot.co, &self.cp, &self.co);
        out.cp = cp;
        out.co = co;
        out
    }

    /// Solved in any of the 24 orientations.
    pub fn is_solved(&self) -> bool {
        tables().rots.iter().any(|r| r.state == *self)
    }

    /// Re-express the state so that every centre is on its home face.
    /// Fails unless the centres are solved up to a whole-cube rotation.
    pub fn with_home_centres(&self) -> Option<Cube> {
        tables().rots.iter().map(|r| self.after_rotation(&r.state)).find(|c| c.ct == SOLVED_CT)
    }

    pub fn wing_parity(&self) -> u8 {
        perm_parity(&self.ep)
    }

    pub fn corner_parity(&self) -> u8 {
        perm_parity(&self.cp)
    }

    pub fn from_facelets(s: &str) -> Result<Cube, String> {
        let b = s.trim().as_bytes();
        if b.len() != 96 {
            return Err(format!("expected 96 facelets, got {}", b.len()));
        }
        let mut f = [0u8; 96];
        for i in 0..96 {
            f[i] = match b"URFDLB".iter().position(|&c| c == b[i]) {
                Some(p) => p as u8,
                None => return Err(format!("bad facelet '{}' at {}", b[i] as char, i)),
            };
        }
        let mut c = Cube::solved();
        for i in 0..24 {
            c.ct[i] = f[CENTER_FACELET[i] as usize];
        }
        for i in 0..24 {
            let a = f[EDGE_FACELET[i][0] as usize];
            let bb = f[EDGE_FACELET[i][1] as usize];
            let j = (0..24)
                .find(|&j| a == EDGE_FACELET[j][0] / 16 && bb == EDGE_FACELET[j][1] / 16)
                .ok_or_else(|| format!("no wing with the colours at wing position {}", i))?;
            c.ep[i] = j as u8;
        }
        for i in 0..8 {
            let ori = (0..3)
                .find(|&o| {
                    let x = f[CORNER_FACELET[i][o] as usize];
                    x == 0 || x == 3
                })
                .ok_or_else(|| format!("corner {} has no U/D facelet", i))?;
            let c1 = f[CORNER_FACELET[i][(ori + 1) % 3] as usize];
            let c2 = f[CORNER_FACELET[i][(ori + 2) % 3] as usize];
            let j = (0..8)
                .find(|&j| c1 == CORNER_FACELET[j][1] / 16 && c2 == CORNER_FACELET[j][2] / 16)
                .ok_or_else(|| format!("no corner with the colours at corner position {}", i))?;
            c.cp[i] = j as u8;
            c.co[i] = ori as u8;
        }
        c.validate()?;
        Ok(c)
    }

    pub fn to_facelets(&self) -> String {
        let mut f = [0u8; 96];
        for i in 0..24 {
            f[CENTER_FACELET[i] as usize] = self.ct[i];
            let p = self.ep[i] as usize;
            f[EDGE_FACELET[i][0] as usize] = EDGE_FACELET[p][0] / 16;
            f[EDGE_FACELET[i][1] as usize] = EDGE_FACELET[p][1] / 16;
        }
        for c in 0..8 {
            let j = self.cp[c] as usize;
            let ori = self.co[c] as usize;
            for n in 0..3 {
                f[CORNER_FACELET[c][(n + ori) % 3] as usize] = CORNER_FACELET[j][n] / 16;
            }
        }
        f.iter().map(|&x| b"URFDLB"[x as usize] as char).collect()
    }

    /// Checks that the pieces form a reachable 4x4x4 state.
    pub fn validate(&self) -> Result<(), String> {
        let mut cnt = [0; 6];
        for &c in &self.ct {
            cnt[c as usize] += 1;
        }
        if cnt != [4; 6] {
            return Err("each colour must appear on exactly 4 centres".into());
        }
        let mut seen = [false; 24];
        for &e in &self.ep {
            if std::mem::replace(&mut seen[e as usize], true) {
                return Err("duplicate wing edge".into());
            }
        }
        let mut seen = [false; 8];
        for &c in &self.cp {
            if std::mem::replace(&mut seen[c as usize], true) {
                return Err("duplicate corner".into());
            }
        }
        if self.co.iter().map(|&x| x as u32).sum::<u32>() % 3 != 0 {
            return Err("corner twist is not solvable".into());
        }
        Ok(())
    }
}

pub fn perm_parity(p: &[u8]) -> u8 {
    let mut seen = [false; 24];
    let mut parity = 0u8;
    for i in 0..p.len() {
        if seen[i] {
            continue;
        }
        let mut j = i;
        let mut len = 0;
        while !seen[j] {
            seen[j] = true;
            j = p[j] as usize;
            len += 1;
        }
        parity ^= (len + 1) & 1;
    }
    parity
}

/// Parses a move sequence such as `U Rw2 f' x`. Whole-cube rotation tokens
/// (`x`, `y`, `z` with suffixes) are ignored: they do not change solvedness.
pub fn parse_moves(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for tok in s.split_whitespace() {
        let b = tok.as_bytes();
        if matches!(b[0], b'x' | b'y' | b'z') {
            continue;
        }
        let mut face = match b[0] {
            b'U' => 0,
            b'R' => 1,
            b'F' => 2,
            b'D' => 3,
            b'L' => 4,
            b'B' => 5,
            b'u' => 6,
            b'r' => 7,
            b'f' => 8,
            b'd' => 9,
            b'l' => 10,
            b'b' => 11,
            _ => return Err(format!("bad move '{}'", tok)),
        };
        let mut power = 0;
        for &ch in &b[1..] {
            match ch {
                b'w' if face < 6 => face += 6,
                b'2' => power = 1,
                b'\'' => power = 2,
                _ => return Err(format!("bad move '{}'", tok)),
            }
        }
        out.push((face * 3 + power) as u8);
    }
    Ok(out)
}

pub fn format_moves(seq: &[u8]) -> String {
    seq.iter().map(|&m| MOVE_NAMES[m as usize]).collect::<Vec<_>>().join(" ")
}

const fn fl(face: u8, k: u8) -> u8 {
    face * 16 + k
}
const U: u8 = 0;
const R: u8 = 1;
const F: u8 = 2;
const D: u8 = 3;
const L: u8 = 4;
const B: u8 = 5;

const CENTER_FACELET: [u8; 24] = [
    fl(U, 5), fl(U, 6), fl(U, 10), fl(U, 9), fl(D, 5), fl(D, 6), fl(D, 10), fl(D, 9),
    fl(F, 5), fl(F, 6), fl(F, 10), fl(F, 9), fl(B, 5), fl(B, 6), fl(B, 10), fl(B, 9),
    fl(R, 5), fl(R, 6), fl(R, 10), fl(R, 9), fl(L, 5), fl(L, 6), fl(L, 10), fl(L, 9),
];

const EDGE_FACELET: [[u8; 2]; 24] = [
    [fl(U, 13), fl(F, 1)], [fl(U, 4), fl(L, 1)], [fl(U, 2), fl(B, 1)], [fl(U, 11), fl(R, 1)],
    [fl(D, 13), fl(B, 14)], [fl(D, 4), fl(L, 14)], [fl(D, 2), fl(F, 14)], [fl(D, 11), fl(R, 14)],
    [fl(L, 11), fl(F, 8)], [fl(L, 4), fl(B, 7)], [fl(R, 11), fl(B, 8)], [fl(R, 4), fl(F, 7)],
    [fl(F, 2), fl(U, 14)], [fl(L, 2), fl(U, 8)], [fl(B, 2), fl(U, 1)], [fl(R, 2), fl(U, 7)],
    [fl(B, 13), fl(D, 14)], [fl(L, 13), fl(D, 8)], [fl(F, 13), fl(D, 1)], [fl(R, 13), fl(D, 7)],
    [fl(F, 4), fl(L, 7)], [fl(B, 11), fl(L, 8)], [fl(B, 4), fl(R, 7)], [fl(F, 11), fl(R, 8)],
];

const CORNER_FACELET: [[u8; 3]; 8] = [
    [fl(U, 15), fl(R, 0), fl(F, 3)], [fl(U, 12), fl(F, 0), fl(L, 3)],
    [fl(U, 0), fl(L, 0), fl(B, 3)], [fl(U, 3), fl(B, 0), fl(R, 3)],
    [fl(D, 3), fl(F, 15), fl(R, 12)], [fl(D, 0), fl(L, 15), fl(F, 12)],
    [fl(D, 12), fl(B, 15), fl(L, 12)], [fl(D, 15), fl(R, 15), fl(B, 12)],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_have_order_four() {
        for f in 0..12u8 {
            let mut c = Cube::solved();
            for _ in 0..4 {
                c.apply(f * 3);
            }
            assert_eq!(c, Cube::solved());
            let mut c = Cube::solved();
            c.apply(f * 3);
            c.apply(f * 3 + 2);
            assert_eq!(c, Cube::solved());
            let mut c = Cube::solved();
            c.apply(f * 3 + 1);
            c.apply(f * 3 + 1);
            assert_eq!(c, Cube::solved());
        }
    }

    #[test]
    fn rotations_and_relabel() {
        let t = tables();
        assert_eq!(t.rots.len(), 24);
        for r in &t.rots {
            assert!(r.state.is_solved());
            assert_eq!(r.state.with_home_centres().unwrap(), Cube::solved());
        }
        // rot, m == relabel(m), rot
        let seq = parse_moves("U Rw2 F' Dw B2 L Uw' R").unwrap();
        for (ri, r) in t.rots.iter().enumerate() {
            let mut a = Cube::solved();
            a.apply_seq(&r.seq);
            a.apply_seq(&seq);
            let mut b = Cube::solved();
            let rel: Vec<u8> = seq.iter().map(|&m| t.relabel[ri][m as usize]).collect();
            b.apply_seq(&rel);
            b.apply_seq(&r.seq);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn after_rotation_matches_moves() {
        let t = tables();
        let seq = parse_moves("U Rw2 F' Dw B2 L Uw' R Fw D'").unwrap();
        let s = Cube::from_moves(&seq);
        for r in &t.rots {
            let mut a = Cube::solved();
            a.apply_seq(&r.seq);
            a.apply_seq(&seq);
            assert_eq!(a, s.after_rotation(&r.state));
        }
    }

    #[test]
    fn facelets_roundtrip() {
        let seq = parse_moves("U Rw2 F' Dw B2 L Uw' R Fw D' Bw Lw2 F R'").unwrap();
        let s = Cube::from_moves(&seq);
        let f = s.to_facelets();
        assert_eq!(Cube::from_facelets(&f).unwrap(), s);
        assert_eq!(
            Cube::solved().to_facelets(),
            "UUUUUUUUUUUUUUUURRRRRRRRRRRRRRRRFFFFFFFFFFFFFFFFDDDDDDDDDDDDDDDDLLLLLLLLLLLLLLLLBBBBBBBBBBBBBBBB"
        );
    }
}
