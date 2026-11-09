//! 3x3x3 two-phase solver for the reduced cube.
//!
//! The cube is described by the paired dedges and corners of the 4x4x4 model,
//! so no convention mapping is needed. Edge flip is "low slot holds a high
//! wing": quarter turns of R and L flip, everything else does not.

use crate::cube::{skip_move, tables as cube_tables, Cube};

pub const N_TWIST: usize = 2187;
pub const N_FLIP: usize = 2048;
pub const N_SLICE: usize = 495;
const N_PERM: usize = 40320;
const N_SPERM: usize = 24;
/// U U2 U' D D2 D' R2 L2 F2 B2
const G1_MOVES: [u8; 10] = [0, 1, 2, 9, 10, 11, 4, 13, 7, 16];
const SLICE_GOAL_MASK: usize = 0xF00;
const UNREACHED: u8 = 255;
pub const UNBOUNDED: usize = 30;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct C3 {
    pub cp: [u8; 8],
    pub co: [u8; 8],
    pub ep: [u8; 12],
    pub eo: [u8; 12],
}

impl C3 {
    pub fn identity() -> C3 {
        C3 {
            cp: [0, 1, 2, 3, 4, 5, 6, 7],
            co: [0; 8],
            ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            eo: [0; 12],
        }
    }

    /// Reads the corners and dedges of a 4x4x4 whose dedges are all paired.
    pub fn from_cube(c: &Cube) -> Option<C3> {
        let mut out = C3::identity();
        for s in 0..12 {
            if c.ep[s + 12] != (c.ep[s] + 12) % 24 {
                return None;
            }
            out.ep[s] = c.ep[s] % 12;
            out.eo[s] = c.ep[s] / 12;
        }
        out.cp = c.cp;
        out.co = c.co;
        Some(out)
    }

    /// `self` followed by `b`.
    pub fn mul(&self, b: &C3) -> C3 {
        let mut out = C3::identity();
        for s in 0..12 {
            let k = b.ep[s] as usize;
            out.ep[s] = self.ep[k];
            out.eo[s] = self.eo[k] ^ b.eo[s];
        }
        for c in 0..8 {
            let k = b.cp[c] as usize;
            out.cp[c] = self.cp[k];
            out.co[c] = (self.co[k] + b.co[c]) % 3;
        }
        out
    }

    pub fn inverse(&self) -> C3 {
        let mut out = C3::identity();
        for s in 0..12 {
            let k = self.ep[s] as usize;
            out.ep[k] = s as u8;
            out.eo[k] = self.eo[s];
        }
        for c in 0..8 {
            let k = self.cp[c] as usize;
            out.cp[k] = c as u8;
            out.co[k] = (3 - self.co[c]) % 3;
        }
        out
    }

    fn twist(&self) -> usize {
        self.co[..7].iter().fold(0, |a, &x| a * 3 + x as usize)
    }

    fn flip(&self) -> usize {
        (0..11).fold(0, |a, i| a | (self.eo[i] as usize) << i)
    }

    fn slice_mask(&self) -> usize {
        (0..12).fold(0, |a, i| a | ((self.ep[i] >= 8) as usize) << i)
    }
}

fn perm_rank(p: &[u8]) -> usize {
    let n = p.len();
    let mut r = 0;
    for i in 0..n {
        let smaller = p[i + 1..].iter().filter(|&&x| x < p[i]).count();
        r = r * (n - i) + smaller;
    }
    r
}

fn perm_unrank(mut r: usize, out: &mut [u8]) {
    let n = out.len();
    let mut digits = [0usize; 12];
    for i in (0..n).rev() {
        digits[i] = r % (n - i);
        r /= n - i;
    }
    let mut avail: Vec<u8> = (0..n as u8).collect();
    for i in 0..n {
        out[i] = avail.remove(digits[i]);
    }
}

pub struct T3 {
    pub(crate) moves: [C3; 18],
    pub(crate) twist_mv: Vec<[u16; 18]>,
    pub(crate) flip_mv: Vec<[u16; 18]>,
    pub(crate) slice_mv: Vec<[u16; 18]>,
    pub(crate) slice_rank: Vec<u16>,
    slice_goal: usize,
    cperm_mv: Vec<[u16; 10]>,
    uperm_mv: Vec<[u16; 10]>,
    sperm_mv: Vec<[u8; 10]>,
    p_ts: Vec<u8>,
    p_fs: Vec<u8>,
    p_tf: Vec<u8>,
    p_cs: Vec<u8>,
    p_us: Vec<u8>,
    /// Frames used to retry the search along the three axes.
    pub(crate) frames: Vec<(usize, C3, C3)>,
}

fn bfs2<const M: usize>(
    na: usize,
    nb: usize,
    amv: impl Fn(usize, usize) -> usize,
    bmv: impl Fn(usize, usize) -> usize,
    goal: (usize, usize),
) -> Vec<u8> {
    let mut dist = vec![UNREACHED; na * nb];
    dist[goal.0 * nb + goal.1] = 0;
    let mut frontier = vec![(goal.0 * nb + goal.1) as u32];
    let mut depth = 0u8;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &x in &frontier {
            let (a, b) = (x as usize / nb, x as usize % nb);
            for m in 0..M {
                let y = amv(a, m) * nb + bmv(b, m);
                if dist[y] == UNREACHED {
                    dist[y] = depth + 1;
                    next.push(y as u32);
                }
            }
        }
        frontier = next;
        depth += 1;
    }
    dist
}

impl T3 {
    pub fn new() -> T3 {
        let ct = cube_tables();
        let mut moves = [C3::identity(); 18];
        for m in 0..18 {
            moves[m] = C3::from_cube(&Cube::from_moves(&[m as u8])).unwrap();
        }
        // Phase 1 coordinates.
        let mut twist_mv = vec![[0u16; 18]; N_TWIST];
        for t in 0..N_TWIST {
            let mut c = C3::identity();
            let (mut x, mut sum) = (t, 0);
            for i in (0..7).rev() {
                c.co[i] = (x % 3) as u8;
                sum += x % 3;
                x /= 3;
            }
            c.co[7] = ((15 - sum) % 3) as u8;
            for m in 0..18 {
                twist_mv[t][m] = c.mul(&moves[m]).twist() as u16;
            }
        }
        let mut flip_mv = vec![[0u16; 18]; N_FLIP];
        for f in 0..N_FLIP {
            let mut c = C3::identity();
            for i in 0..11 {
                c.eo[i] = ((f >> i) & 1) as u8;
            }
            c.eo[11] = (f.count_ones() & 1) as u8;
            for m in 0..18 {
                flip_mv[f][m] = c.mul(&moves[m]).flip() as u16;
            }
        }
        let mut slice_rank = vec![u16::MAX; 4096];
        let mut slice_unrank = Vec::new();
        for x in 0..4096usize {
            if x.count_ones() == 4 {
                slice_rank[x] = slice_unrank.len() as u16;
                slice_unrank.push(x);
            }
        }
        assert_eq!(slice_unrank.len(), N_SLICE);
        let mut slice_mv = vec![[0u16; 18]; N_SLICE];
        for s in 0..N_SLICE {
            let mut c = C3::identity();
            let (mut hi, mut lo) = (8u8, 0u8);
            for i in 0..12 {
                if (slice_unrank[s] >> i) & 1 == 1 {
                    c.ep[i] = hi;
                    hi += 1;
                } else {
                    c.ep[i] = lo;
                    lo += 1;
                }
            }
            for m in 0..18 {
                slice_mv[s][m] = slice_rank[c.mul(&moves[m]).slice_mask()];
            }
        }
        let slice_goal = slice_rank[SLICE_GOAL_MASK] as usize;
        // Phase 2 coordinates.
        let mut cperm_mv = vec![[0u16; 10]; N_PERM];
        let mut uperm_mv = vec![[0u16; 10]; N_PERM];
        for p in 0..N_PERM {
            let mut c = C3::identity();
            perm_unrank(p, &mut c.cp);
            perm_unrank(p, &mut c.ep[..8]);
            for (k, &m) in G1_MOVES.iter().enumerate() {
                let d = c.mul(&moves[m as usize]);
                cperm_mv[p][k] = perm_rank(&d.cp) as u16;
                uperm_mv[p][k] = perm_rank(&d.ep[..8]) as u16;
            }
        }
        let mut sperm_mv = vec![[0u8; 10]; N_SPERM];
        for p in 0..N_SPERM {
            let mut c = C3::identity();
            let mut s = [0u8; 4];
            perm_unrank(p, &mut s);
            for i in 0..4 {
                c.ep[8 + i] = s[i] + 8;
            }
            for (k, &m) in G1_MOVES.iter().enumerate() {
                let d = c.mul(&moves[m as usize]);
                let mut s = [0u8; 4];
                for i in 0..4 {
                    s[i] = d.ep[8 + i] - 8;
                }
                sperm_mv[p][k] = perm_rank(&s) as u8;
            }
        }
        let tm = |a: usize, m: usize| twist_mv[a][m] as usize;
        let fm = |a: usize, m: usize| flip_mv[a][m] as usize;
        let sm = |a: usize, m: usize| slice_mv[a][m] as usize;
        let p_ts = bfs2::<18>(N_TWIST, N_SLICE, tm, sm, (0, slice_goal));
        let p_fs = bfs2::<18>(N_FLIP, N_SLICE, fm, sm, (0, slice_goal));
        let p_tf = bfs2::<18>(N_TWIST, N_FLIP, tm, fm, (0, 0));
        let cm = |a: usize, m: usize| cperm_mv[a][m] as usize;
        let um = |a: usize, m: usize| uperm_mv[a][m] as usize;
        let spm = |a: usize, m: usize| sperm_mv[a][m] as usize;
        let p_cs = bfs2::<10>(N_PERM, N_SPERM, cm, spm, (0, 0));
        let p_us = bfs2::<10>(N_PERM, N_SPERM, um, spm, (0, 0));
        // Frames: identity and the two rotations about the URF-DBL diagonal.
        let x = [21u8, 32];
        let y = [18u8, 29];
        let c1: Vec<u8> = x.iter().chain(y.iter()).copied().collect();
        let c2: Vec<u8> = c1.iter().chain(c1.iter()).copied().collect();
        let mut frames = Vec::new();
        for seq in [vec![], c1, c2] {
            let r = ct.rot_index(&seq);
            let rot = C3::from_cube(&ct.rots[r].state).unwrap();
            frames.push((r, rot.inverse(), rot));
        }
        T3 {
            moves,
            twist_mv,
            flip_mv,
            slice_mv,
            slice_rank,
            cperm_mv,
            uperm_mv,
            sperm_mv,
            p_ts,
            p_fs,
            p_tf,
            p_cs,
            p_us,
            slice_goal,
            frames,
        }
    }

    /// Slice coordinate of the solved cube; also the index of the goal in the exact table.
    pub(crate) fn slice_goal(&self) -> usize {
        self.slice_goal
    }

    pub(crate) fn coords(&self, c: &C3) -> (usize, usize, usize) {
        (c.twist(), c.flip(), self.slice_rank[c.slice_mask()] as usize)
    }

    /// Lower bound on the distance to the domino subgroup, from three small tables.
    #[inline(always)]
    pub(crate) fn h1(&self, t: usize, f: usize, s: usize) -> u8 {
        self.p_ts[t * N_SLICE + s].max(self.p_fs[f * N_SLICE + s]).max(self.p_tf[t * N_FLIP + f])
    }

    #[inline(always)]
    fn h2(&self, c: usize, u: usize, s: usize) -> u8 {
        self.p_cs[c * N_SPERM + s].max(self.p_us[u * N_SPERM + s])
    }

    /// Finds a short solution (outer moves, 0..18). `probes` is the minimum
    /// number of phase-2 attempts before the best solution so far is returned.
    pub fn solve(&self, cube: &C3, probes: u64) -> Vec<u8> {
        self.solve_bounded(cube, UNBOUNDED, probes).expect("3x3x3 state must be solvable")
    }

    /// Like `solve`, but only accepts solutions of at most `max_len` moves and
    /// gives up after `probes` phase-2 attempts without one.
    pub fn solve_bounded(&self, cube: &C3, max_len: usize, probes: u64) -> Option<Vec<u8>> {
        let ct = cube_tables();
        let mut variants = Vec::new();
        for (r, rinv, rot) in &self.frames {
            let conj = rinv.mul(cube).mul(rot);
            variants.push((conj, *r, false));
            variants.push((conj.inverse(), *r, true));
        }
        let mut s = Search {
            t: self,
            start: C3::identity(),
            path: [0; 40],
            best: None,
            best_len: max_len + 1,
            bounded: max_len < UNBOUNDED,
            found: false,
            probes: 0,
            min_probes: probes,
            stop: false,
        };
        let mut result: Option<Vec<u8>> = None;
        'outer: for d1 in 0..=20usize {
            for (c, r, inverse) in &variants {
                if d1 >= s.best_len {
                    break 'outer;
                }
                let (tw, fl, sl) = (c.twist(), c.flip(), self.slice_rank[c.slice_mask()] as usize);
                if self.h1(tw, fl, sl) as usize > d1 {
                    continue;
                }
                s.start = *c;
                s.best = None;
                s.phase1(tw, fl, sl, d1, 255, 0);
                if let Some(mut sol) = s.best.take() {
                    if *inverse {
                        sol = crate::cube::inv_seq(&sol);
                    }
                    for m in sol.iter_mut() {
                        *m = ct.relabel[*r][*m as usize];
                    }
                    result = Some(sol);
                }
                if s.stop {
                    break 'outer;
                }
            }
        }
        result
    }
}

struct Search<'a> {
    t: &'a T3,
    start: C3,
    path: [u8; 40],
    best: Option<Vec<u8>>,
    best_len: usize,
    bounded: bool,
    found: bool,
    probes: u64,
    min_probes: u64,
    stop: bool,
}

impl Search<'_> {
    fn phase1(&mut self, tw: usize, fl: usize, sl: usize, rem: usize, last: u8, depth: usize) {
        if self.stop {
            return;
        }
        if rem == 0 {
            // A phase-1 solution ending in a G1 move is found with a shorter prefix.
            if depth > 0 && G1_MOVES.contains(&last) {
                return;
            }
            self.phase2_start(depth, last);
            return;
        }
        let t = self.t;
        for m in 0..18u8 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let (a, b, c) = (
                t.twist_mv[tw][m as usize] as usize,
                t.flip_mv[fl][m as usize] as usize,
                t.slice_mv[sl][m as usize] as usize,
            );
            if t.h1(a, b, c) as usize > rem - 1 {
                continue;
            }
            self.path[depth] = m;
            self.phase1(a, b, c, rem - 1, m, depth + 1);
            if self.stop {
                return;
            }
        }
    }

    fn phase2_start(&mut self, d1: usize, last: u8) {
        let t = self.t;
        let mut c = self.start;
        for &m in &self.path[..d1] {
            c = c.mul(&t.moves[m as usize]);
        }
        debug_assert!(c.twist() == 0 && c.flip() == 0 && c.slice_mask() == SLICE_GOAL_MASK);
        let cp = perm_rank(&c.cp);
        let up = perm_rank(&c.ep[..8]);
        let mut s = [0u8; 4];
        for i in 0..4 {
            s[i] = c.ep[8 + i] - 8;
        }
        let sp = perm_rank(&s);
        self.probes += 1;
        if self.best_len <= d1 {
            return;
        }
        let max2 = (self.best_len - 1 - d1).min(12);
        let h = t.h2(cp, up, sp) as usize;
        for d2 in h..=max2 {
            if self.phase2(cp, up, sp, d2, last, d1) {
                self.best_len = d1 + d2;
                self.best = Some(self.path[..d1 + d2].to_vec());
                self.found = true;
                break;
            }
        }
        if self.probes >= self.min_probes && (self.found || self.bounded) {
            self.stop = true;
        }
    }

    fn phase2(&mut self, cp: usize, up: usize, sp: usize, rem: usize, last: u8, depth: usize) -> bool {
        if rem == 0 {
            return cp == 0 && up == 0 && sp == 0;
        }
        let t = self.t;
        for (k, &m) in G1_MOVES.iter().enumerate() {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let (a, b, c) =
                (t.cperm_mv[cp][k] as usize, t.uperm_mv[up][k] as usize, t.sperm_mv[sp][k] as usize);
            if t.h2(a, b, c) as usize > rem - 1 {
                continue;
            }
            self.path[depth] = m;
            if self.phase2(a, b, c, rem - 1, m, depth + 1) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::parse_moves;

    #[test]
    fn solves_outer_move_scrambles() {
        let t = T3::new();
        for scr in [
            "R U R' U' F2 D L' B U2 R D' F L2 B' U",
            "F R U' B2 L D F' R2 U B D2 L' F U2 R' B L U' D F2",
            "U",
            "",
        ] {
            let seq = parse_moves(scr).unwrap();
            let cube = Cube::from_moves(&seq);
            let c3 = C3::from_cube(&cube).unwrap();
            let sol = t.solve(&c3, 200);
            let mut c = cube;
            c.apply_seq(&sol);
            assert_eq!(c, Cube::solved(), "scramble {}", scr);
            assert!(sol.len() <= 22);
        }
    }
}
