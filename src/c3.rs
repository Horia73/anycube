//! 3x3x3 two-phase solver for the reduced cube.
//!
//! The cube is described by the paired dedges and corners of the 4x4x4 model,
//! so no convention mapping is needed. Edge flip is "low slot holds a high
//! wing": quarter turns of R and L flip, everything else does not.

use crate::big1::Big1;
use crate::cache::{Reader, Writer};
use crate::cube::{skip_move, tables as cube_tables, Cube};
use crate::sym::{closure, conjugators};
use crate::tables::{load_or_gen, table_dir, Bytes};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

pub const N_TWIST: usize = 2187;
pub const N_FLIP: usize = 2048;
pub const N_SLICE: usize = 495;
const N_PERM: usize = 40320;
const N_SPERM: usize = 24;
/// U U2 U' D D2 D' R2 L2 F2 B2
const G1_MOVES: [u8; 10] = [0, 1, 2, 9, 10, 11, 4, 13, 7, 16];
const SLICE_GOAL_MASK: usize = 0xF00;
const UNREACHED: u8 = 255;
const CACHE_FILE: &str = "cube333.bin";

// Phase 2, corners and U/D edges together: the exact number of moves to
// put both in place, for every pair of their permutations within `CE_DEPTH`
// moves of solved. The slice edges are left out, so it is a lower bound for
// the phase, but a close one: the two small tables the search used before
// each see one of the two permutations with the slice, and their maximum is
// far below the truth for most positions, which made the second phase the
// larger part of the time a 3x3x3 takes.
//
// A corner permutation and its images under the sixteen symmetries that
// keep the U/D axis are equally far, so the table has one row per class of
// corner permutations (2,768 of them) and the edges are turned along.
// One file: class of every corner permutation, the symmetry that takes it
// to its class's first member, the edge permutations under each symmetry,
// and the distances, four bits each.
const N_CLASS: usize = 2768;
const CE_DEPTH: u8 = 13;
const CE_FAR: u8 = 15;
const CE_CLASS: usize = 0;
const CE_SYM: usize = CE_CLASS + 2 * N_PERM;
const CE_CONJ: usize = CE_SYM + N_PERM;
const CE_TABLE: usize = CE_CONJ + 2 * 16 * N_PERM;
const CE_LEN: usize = CE_TABLE + N_CLASS * N_PERM / 2;
const CE_FILE: &str = "phase2-333.bin";

/// The moves of the second phase renamed by a symmetry of the cube that
/// keeps the U/D axis: where each face goes (U R F D L B) and whether the
/// turns change direction.
fn g1_renamed(faces: [u8; 6], mirrored: bool) -> Vec<(usize, usize)> {
    (0..G1_MOVES.len())
        .map(|k| {
            let m = G1_MOVES[k];
            let power = if mirrored { 2 - m % 3 } else { m % 3 };
            let to = faces[(m / 3) as usize] * 3 + power;
            (k, G1_MOVES.iter().position(|&x| x == to).expect("the symmetry keeps the moves of phase 2"))
        })
        .collect()
}

fn gen_corners_edges(cperm_mv: &[[u16; 10]], uperm_mv: &[[u16; 10]]) -> Vec<u8> {
    // A quarter turn of the cube about the U/D axis, a half turn about the R/L axis, the mirror image left to right.
    let renamings = [g1_renamed([0, 5, 1, 3, 2, 4], false), g1_renamed([3, 1, 5, 0, 4, 2], false), g1_renamed([0, 4, 2, 3, 1, 5], true)];
    let corner = |k: usize, x: usize| cperm_mv[x][k] as usize;
    let edge = |k: usize, x: usize| uperm_mv[x][k] as usize;
    let generators: Vec<Vec<Vec<u32>>> = renamings
        .iter()
        .map(|pairs| {
            let c = conjugators(N_PERM, pairs, &corner, &corner, &[(0, 0)]);
            let u = conjugators(N_PERM, pairs, &edge, &edge, &[(0, 0)]);
            assert!(c.len() == 1 && u.len() == 1, "one map of the corners and one of the edges per symmetry");
            vec![c[0].clone(), u[0].clone()]
        })
        .collect();
    let syms = closure(&generators);
    assert_eq!(syms.len(), 16, "symmetries of phase 2");
    // Classes of corner permutations: the smallest image names the class.
    let best: Vec<(u32, u8)> =
        (0..N_PERM).map(|c| (0..16).map(|s| (syms[s][0][c], s as u8)).min().unwrap()).collect();
    let mut reps: Vec<u32> = best.iter().map(|b| b.0).collect();
    reps.sort_unstable();
    reps.dedup();
    assert_eq!(reps.len(), N_CLASS, "classes of corner permutations");
    let class: Vec<u16> = best.iter().map(|b| reps.binary_search(&b.0).unwrap() as u16).collect();
    // The symmetries that keep a class's first member in place: its states have that many names.
    let stab: Vec<u16> =
        reps.iter().map(|&r| (0..16).fold(0u16, |a, s| a | ((syms[s][0][r as usize] == r) as u16) << s)).collect();
    let cells: Vec<AtomicU8> = (0..N_CLASS * N_PERM / 2).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    let get = |i: usize| (cells[i >> 1].load(Relaxed) >> ((i & 1) * 4)) & 15;
    let put = |c: usize, u: usize, v: u8| -> u64 {
        let mut new = 0;
        let mut st = stab[c];
        while st != 0 {
            let s = st.trailing_zeros() as usize;
            st &= st - 1;
            let i = c * N_PERM + syms[s][1][u] as usize;
            let (cell, shift) = (&cells[i >> 1], (i & 1) * 4);
            let mut cur = cell.load(Relaxed);
            while (cur >> shift) & 15 == CE_FAR {
                match cell.compare_exchange_weak(cur, (cur & !(15 << shift)) | v << shift, Relaxed, Relaxed) {
                    Ok(_) => {
                        new += 1;
                        break;
                    }
                    Err(now) => cur = now,
                }
            }
        }
        new
    };
    put(class[0] as usize, syms[best[0].1 as usize][1][0] as usize, 0);
    let mut total = 1u64;
    for depth in 0..CE_DEPTH {
        let new: u64 = (0..N_CLASS)
            .into_par_iter()
            .map(|c| {
                let mut new = 0;
                for u in 0..N_PERM {
                    if get(c * N_PERM + u) != depth {
                        continue;
                    }
                    for k in 0..10 {
                        let (c2, u2) = (cperm_mv[reps[c] as usize][k] as usize, uperm_mv[u][k] as usize);
                        new += put(class[c2] as usize, syms[best[c2].1 as usize][1][u2] as usize, depth + 1);
                    }
                }
                new
            })
            .sum();
        total += new;
        eprintln!("  3x3x3 phase 2 table: depth {:2}  {:>10} new  {:>10} total", depth + 1, new, total);
    }
    let mut out = Vec::with_capacity(CE_LEN);
    out.extend(class.iter().flat_map(|c| c.to_le_bytes()));
    out.extend(best.iter().map(|b| b.1));
    for s in 0..16 {
        out.extend(syms[s][1].iter().flat_map(|&u| (u as u16).to_le_bytes()));
    }
    out.extend(cells.into_iter().map(|c| c.into_inner()));
    out
}

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

    pub(crate) fn twist(&self) -> usize {
        self.co[..7].iter().fold(0, |a, &x| a * 3 + x as usize)
    }

    pub(crate) fn flip(&self) -> usize {
        (0..11).fold(0, |a, i| a | (self.eo[i] as usize) << i)
    }

    pub(crate) fn slice_mask(&self) -> usize {
        (0..12).fold(0, |a, i| a | ((self.ep[i] >= 8) as usize) << i)
    }
}

pub(crate) fn perm_rank(p: &[u8]) -> usize {
    let n = p.len();
    let mut r = 0;
    for i in 0..n {
        let smaller = p[i + 1..].iter().filter(|&&x| x < p[i]).count();
        r = r * (n - i) + smaller;
    }
    r
}

pub(crate) fn perm_unrank(mut r: usize, out: &mut [u8]) {
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
    pub(crate) slice_unrank: Vec<usize>,
    slice_goal: usize,
    cperm_mv: Vec<[u16; 10]>,
    uperm_mv: Vec<[u16; 10]>,
    sperm_mv: Vec<[u8; 10]>,
    p_ts: Vec<u8>,
    p_fs: Vec<u8>,
    p_tf: Vec<u8>,
    p_cs: Vec<u8>,
    p_us: Vec<u8>,
    /// Phase 2: corners and U/D edges together; see `gen_corners_edges`.
    ce: Bytes,
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
    /// Loads the tables from the cache, or builds and caches them.
    pub fn new() -> T3 {
        if let Some(t) = T3::from_cache() {
            return t;
        }
        let t = T3::build();
        let mut w = Writer::new();
        w.put(&t.twist_mv);
        w.put(&t.flip_mv);
        w.put(&t.slice_mv);
        w.put(&t.slice_rank);
        w.put(&t.slice_unrank);
        w.put(&t.cperm_mv);
        w.put(&t.uperm_mv);
        w.put(&t.sperm_mv);
        w.put(&t.p_ts);
        w.put(&t.p_fs);
        w.put(&t.p_tf);
        w.put(&t.p_cs);
        w.put(&t.p_us);
        w.save(CACHE_FILE);
        t
    }

    fn from_cache() -> Option<T3> {
        let mut r = Reader::open(CACHE_FILE)?;
        let twist_mv = r.get(N_TWIST)?;
        let flip_mv = r.get(N_FLIP)?;
        let slice_mv = r.get(N_SLICE)?;
        let slice_rank: Vec<u16> = r.get(4096)?;
        let slice_unrank = r.get(N_SLICE)?;
        let cperm_mv = r.get(N_PERM)?;
        let uperm_mv = r.get(N_PERM)?;
        let sperm_mv = r.get(N_SPERM)?;
        let p_ts = r.get(N_TWIST * N_SLICE)?;
        let p_fs = r.get(N_FLIP * N_SLICE)?;
        let p_tf = r.get(N_TWIST * N_FLIP)?;
        let p_cs = r.get(N_PERM * N_SPERM)?;
        let p_us = r.get(N_PERM * N_SPERM)?;
        let slice_goal = slice_rank[SLICE_GOAL_MASK] as usize;
        let ce = T3::corners_edges(&cperm_mv, &uperm_mv);
        Some(T3 {
            ce,
            moves: T3::base_moves(),
            twist_mv,
            flip_mv,
            slice_mv,
            slice_rank,
            slice_unrank,
            cperm_mv,
            uperm_mv,
            sperm_mv,
            p_ts,
            p_fs,
            p_tf,
            p_cs,
            p_us,
            slice_goal,
            frames: T3::base_frames(),
        })
    }

    fn corners_edges(cperm_mv: &[[u16; 10]], uperm_mv: &[[u16; 10]]) -> Bytes {
        load_or_gen(&table_dir().join(CE_FILE), CE_LEN, || gen_corners_edges(cperm_mv, uperm_mv))
    }

    /// Moves to put the corners and the U/D edges in place together, exactly
    /// up to `CE_DEPTH`; `CE_FAR` stands for more.
    #[inline(always)]
    fn corners_and_edges(&self, c: usize, u: usize) -> u8 {
        let d = &self.ce[..];
        let class = u16::from_le_bytes([d[CE_CLASS + 2 * c], d[CE_CLASS + 2 * c + 1]]) as usize;
        let at = CE_CONJ + 2 * (d[CE_SYM + c] as usize * N_PERM + u);
        let i = class * N_PERM + u16::from_le_bytes([d[at], d[at + 1]]) as usize;
        (d[CE_TABLE + (i >> 1)] >> ((i & 1) * 4)) & 15
    }

    fn base_moves() -> [C3; 18] {
        let mut moves = [C3::identity(); 18];
        for m in 0..18 {
            moves[m] = C3::from_cube(&Cube::from_moves(&[m as u8])).unwrap();
        }
        moves
    }

    fn base_frames() -> Vec<(usize, C3, C3)> {
        let ct = cube_tables();
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
        frames
    }

    fn build() -> T3 {
        let moves = T3::base_moves();
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
        let frames = T3::base_frames();
        let ce = T3::corners_edges(&cperm_mv, &uperm_mv);
        T3 {
            ce,
            moves,
            twist_mv,
            flip_mv,
            slice_mv,
            slice_rank,
            slice_unrank,
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
        self.p_cs[c * N_SPERM + s].max(self.p_us[u * N_SPERM + s]).max(self.corners_and_edges(c, u))
    }

    /// Finds a short solution (outer moves, 0..18). `probes` is the minimum
    /// number of phase-2 attempts before the best solution so far is returned.
    ///
    /// With `exact`, the table of exact distances to the domino subgroup, the
    /// first phase follows only sequences that really reach the subgroup in time.
    pub fn solve(&self, cube: &C3, probes: u64, exact: Option<&Big1>) -> Vec<u8> {
        self.solve_bounded(cube, UNBOUNDED, probes, exact).expect("3x3x3 state must be solvable")
    }

    /// Like `solve`, but only accepts solutions of at most `max_len` moves and
    /// gives up after `probes` phase-2 attempts without one.
    pub fn solve_bounded(&self, cube: &C3, max_len: usize, probes: u64, exact: Option<&Big1>) -> Option<Vec<u8>> {
        let ct = cube_tables();
        let mut variants = Vec::new();
        for (r, rinv, rot) in &self.frames {
            let conj = rinv.mul(cube).mul(rot);
            variants.push((conj, *r, false));
            variants.push((conj.inverse(), *r, true));
        }
        let mut s = Search {
            t: self,
            big: exact,
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
                let d = exact.map_or(0, |big| big.exact(self, tw, fl, sl));
                if d as usize > d1 {
                    continue;
                }
                s.start = *c;
                s.best = None;
                s.phase1(tw, fl, sl, d, d1, 255, 0);
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
    big: Option<&'a Big1>,
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
    fn phase1(&mut self, tw: usize, fl: usize, sl: usize, d: u8, rem: usize, last: u8, depth: usize) {
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
            // The last move of the first phase must leave the domino subgroup's moves.
            if rem == 1 && G1_MOVES.contains(&m) {
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
            // The small tables let some dead ends through; the exact table does not.
            let mut dc = 0;
            if let Some(big) = self.big {
                let v = big.get(Big1::index(a, b, c));
                dc = d + (v + 3 - (d + 2) % 3) % 3 - 1;
                if dc as usize > rem - 1 {
                    continue;
                }
            }
            self.path[depth] = m;
            self.phase1(a, b, c, dc, rem - 1, m, depth + 1);
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
            // The small tables are in the cache and turn most moves away; the large one is asked only then.
            if t.p_cs[a * N_SPERM + c].max(t.p_us[b * N_SPERM + c]) as usize > rem - 1 {
                continue;
            }
            if t.corners_and_edges(a, b) as usize > rem - 1 {
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

    /// The table of corners and U/D edges: zero only when both are in
    /// place, never more than one apart across a move, and from every
    /// position within its depth some move leads one nearer. Together these
    /// make its entries the exact distances.
    #[test]
    fn corners_and_edges_are_exact() {
        let t = T3::new();
        assert_eq!(t.corners_and_edges(0, 0), 0);
        let mut seed = 7u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        for round in 0..20_000 {
            // Half of the positions near solved, half anywhere.
            let (mut c, mut u) = (0, 0);
            for _ in 0..if round % 2 == 0 { rnd(12) } else { 40 } {
                let k = rnd(10);
                (c, u) = (t.cperm_mv[c][k] as usize, t.uperm_mv[u][k] as usize);
            }
            let d = t.corners_and_edges(c, u);
            assert_eq!(d == 0, c == 0 && u == 0);
            let next: Vec<u8> =
                (0..10).map(|k| t.corners_and_edges(t.cperm_mv[c][k] as usize, t.uperm_mv[u][k] as usize)).collect();
            if d <= CE_DEPTH {
                assert!(next.iter().all(|&n| n + 1 >= d && n <= d + 1 || n == CE_FAR && d == CE_DEPTH));
                assert!(d == 0 || next.iter().any(|&n| n + 1 == d));
            } else {
                assert!(next.iter().all(|&n| n >= CE_DEPTH));
            }
        }
    }

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
            let sol = t.solve(&c3, 200, None);
            let mut c = cube;
            c.apply_seq(&sol);
            assert_eq!(c, Cube::solved(), "scramble {}", scr);
            assert!(sol.len() <= 22);
        }
    }
}
