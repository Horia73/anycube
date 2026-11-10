//! 6x6x6, first part: every centre onto a face of its own axis, and both
//! wing orbits into even permutations.
//!
//! The four centre orbits are the inner x centres, the outer x centres and
//! the two mirror orbits of oblique centres. An oblique and its mate (the
//! oblique next to it across the middle of a row) move as one piece under
//! 1- and 2-layer turns, like the t centre of a 5x5x5; only 3-layer turns
//! separate mates. Four steps, each keeping what the ones before achieved:
//!
//! 1. R/L inner x centres onto R and L, and R/L obliques into pairs (every
//!    oblique next to a mate of its own axis), with all 54 moves;
//! 2. R/L outer x centres and oblique pairs onto R and L with 1- and
//!    2-layer turns: phase A of the 5x5x5;
//! 3. U/D inner x centres onto U and D, inner wings even, U/D obliques into
//!    pairs (an exact table for each of the two halves bounds the search);
//! 4. U/D outer x centres and oblique pairs onto U and D, outer wings even:
//!    phase B of the 5x5x5.

use crate::cube::Cube;
use crate::cuben::Geometry;
use crate::orbits::{centre_orbits, CentreOrbit};
use crate::stage5::Stage5;
use crate::tables::{gen_stage, load_or_gen, load_or_gen_quiet, table_dir, BitPerm, Bytes, Ranks};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering::Relaxed};

const GOAL_RL: u32 = 0xFF_0000;
const GOAL_UD: u32 = 0x00_00FF;
const N_UD: usize = 12870;
const FAR: u8 = 255;

/// Orbit indices, in the order `centre_orbits` lists them for a 6x6x6.
pub const X_OUT: usize = 0;
pub const OBL_A: usize = 1;
pub const OBL_B: usize = 2;
pub const X_IN: usize = 3;

/// True if move `cur` need not be tried after `prev`: the same face and depth
/// again, or a turn about the same axis that could have come first.
#[inline(always)]
pub fn skip54(prev: u8, cur: u8) -> bool {
    let (a, b) = (prev / 3, cur / 3);
    a == b || (a % 3 == b % 3 && a > b)
}

/// Move numbers: `face * 3 + power` for faces U R F D L B, plus 18 per extra layer.
fn moves_where(keep: impl Fn(usize, usize, usize) -> bool) -> Vec<u8> {
    (0..54u8).filter(|&m| keep(m as usize / 3 % 6, m as usize / 18 + 1, m as usize % 3)).collect()
}

pub struct Stage6 {
    pub g: Geometry,
    pub orbits: Vec<CentreOrbit>,
    pub perms: Vec<BitPerm>,
    ranks: Ranks,
    /// Step 1: moves until the R/L inner x centres are on R and L, by mask,
    /// and a lower bound on the moves that pair the obliques, by the mask of
    /// places where an oblique and its mate differ.
    in_rl: Bytes,
    unpaired: Bytes,
    /// Step 3: its moves; exact moves for (U/D inner x centres among 16
    /// places, inner wing parity); exact moves to pair the U/D obliques of
    /// both orbits among 16 places each.
    m3: Vec<u8>,
    in_ud_mv: Vec<Vec<u16>>,
    in_ud: Vec<u8>,
    obl_ud_mv: [Vec<Vec<u16>>; 2],
    obl_ud: Bytes,
}

/// A staging: the moves and how many each step took.
#[derive(Clone)]
pub struct Staged6 {
    pub moves: Vec<u8>,
    pub lens: [usize; 4],
}

impl Stage6 {
    pub fn load() -> Stage6 {
        let g = Geometry::new(6);
        let orbits = centre_orbits(&g);
        assert_eq!(orbits.len(), 4);
        let perms: Vec<BitPerm> = orbits.iter().map(|o| BitPerm::from_src(&o.src)).collect();
        let ranks = Ranks::new();
        let dir = table_dir();
        let side = |f: usize| f != 1 && f != 4;

        let all: Vec<u8> = (0..54).collect();
        let in_rl = load_or_gen_quiet(&dir.join("stage666-1.bin"), 1 << 24, &|| gen_stage(&perms[X_IN], GOAL_RL, &all));
        let unpaired =
            load_or_gen(&dir.join("stage666-1-pairs.bin"), 1 << 24, || gen_unpaired(&orbits[OBL_A], &orbits[OBL_B], &perms[OBL_A]));

        let table = |o: usize, moves: &[u8]| -> Vec<Vec<u16>> {
            (0..N_UD)
                .map(|r| {
                    moves
                        .iter()
                        .map(|&m| {
                            let y = perms[o].apply(m, ranks.unrank16[r] as u32);
                            assert!(y < 1 << 16, "these moves keep the R/L centres in place");
                            ranks.rank16[y as usize]
                        })
                        .collect()
                })
                .collect()
        };
        // Step 3: U F D B face turns, turns of R and L of two or three layers,
        // half turns of two or three layers of the rest.
        let m3 = moves_where(|f, layers, power| match layers {
            1 => side(f),
            _ => !side(f) || power == 1,
        });
        let in_ud_mv = table(X_IN, &m3);
        let goal = ranks.rank16[GOAL_UD as usize] as usize;
        let mut in_ud = vec![FAR; N_UD * 2];
        in_ud[goal * 2] = 0;
        let mut frontier = vec![goal * 2];
        let mut depth = 0;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &s in &frontier {
                for (k, &m) in m3.iter().enumerate() {
                    let t = in_ud_mv[s / 2][k] as usize * 2 + (s % 2 ^ flips_inner(m) as usize);
                    if in_ud[t] == FAR {
                        in_ud[t] = depth + 1;
                        next.push(t);
                    }
                }
            }
            frontier = next;
            depth += 1;
        }
        assert!(in_ud.iter().all(|&d| d != FAR), "step 3 reaches every state");

        let obl_ud_mv = [table(OBL_A, &m3), table(OBL_B, &m3)];
        let obl_ud = load_or_gen(&dir.join("stage666-3-pairs.bin"), N_UD * N_UD, || {
            let dist: Vec<AtomicU8> = (0..N_UD * N_UD).into_par_iter().map(|_| AtomicU8::new(FAR)).collect();
            // Mates have the same number, so paired means the two orbits look alike.
            for a in 0..N_UD {
                dist[a * N_UD + a].store(0, Relaxed);
            }
            for depth in 0..FAR - 1 {
                let n: u64 = (0..N_UD)
                    .into_par_iter()
                    .map(|a| {
                        let mut new = 0;
                        for b in 0..N_UD {
                            if dist[a * N_UD + b].load(Relaxed) != depth {
                                continue;
                            }
                            for k in 0..m3.len() {
                                let j = obl_ud_mv[0][a][k] as usize * N_UD + obl_ud_mv[1][b][k] as usize;
                                if dist[j].load(Relaxed) == FAR
                                    && dist[j].compare_exchange(FAR, depth + 1, Relaxed, Relaxed).is_ok()
                                {
                                    new += 1;
                                }
                            }
                        }
                        new
                    })
                    .sum();
                eprintln!("  6x6x6 step 3 pairing table: depth {:2}  {:>10} new", depth + 1, n);
                if n == 0 {
                    break;
                }
            }
            let mut v = std::mem::ManuallyDrop::new(dist);
            // AtomicU8 has the same layout as u8.
            unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
        });
        Stage6 { g, orbits, perms, ranks, in_rl, unpaired, m3, in_ud_mv, in_ud, obl_ud_mv, obl_ud }
    }

    /// Positions of the centres of one axis colour pair (0 U/D, 1 R/L, 2 F/B) in an orbit.
    pub fn mask(&self, orbit: usize, facelets: &[u8], axis: u8) -> u32 {
        let col = self.orbits[orbit].colours(facelets);
        (0..24).fold(0, |m, i| m | ((col[i] % 3 == axis) as u32) << i)
    }

    /// Parity of the permutation of the wings in the given layers (inner: 2 and 3; outer: 1 and 4).
    fn wings_odd(&self, facelets: &[u8], layers: [usize; 4]) -> bool {
        let sub = self.g.to_string(&self.g.sub_facelets(facelets, &layers));
        Cube::from_facelets(&sub).expect("four layers of a 6x6x6 form a 4x4x4").wing_parity() == 1
    }

    pub fn is_staged(&self, facelets: &[u8]) -> bool {
        (0..4).all(|o| self.mask(o, facelets, 1) == GOAL_RL && self.mask(o, facelets, 0) == GOAL_UD)
            && !self.wings_odd(facelets, [0, 2, 3, 5])
            && !self.wings_odd(facelets, [0, 1, 4, 5])
    }

    /// The best stagings found, shortest first. `width` candidates are
    /// carried from step to step, each continued in up to `width` ways.
    pub fn stage(&self, s5: &Stage5, facelets: &[u8], width: usize) -> Vec<Staged6> {
        let start = Staged6 { moves: Vec::new(), lens: [0; 4] };
        let mut cands = vec![(start, facelets.to_vec())];
        for step in 0..4 {
            let mut next: Vec<(Staged6, Vec<u8>)> = cands
                .par_iter()
                .flat_map_iter(|(c, f)| {
                    let sols = self.step(s5, step, f, width);
                    sols.into_iter().map(move |sol| {
                        let mut f2 = f.clone();
                        self.g.apply_seq(&mut f2, &sol.iter().map(|&m| m as usize).collect::<Vec<_>>());
                        let mut c2 = c.clone();
                        c2.lens[step] = sol.len();
                        c2.moves.extend(sol);
                        (c2, f2)
                    })
                })
                .collect();
            next.sort_by_key(|c| c.0.moves.len());
            next.truncate(width);
            cands = next;
        }
        debug_assert!(cands.iter().all(|c| self.is_staged(&c.1)));
        cands.into_iter().map(|c| c.0).collect()
    }

    /// Solutions of one step from a cube on which the earlier steps are done.
    fn step(&self, s5: &Stage5, step: usize, f: &[u8], cap: usize) -> Vec<Vec<u8>> {
        match step {
            0 => self.step1([self.mask(X_IN, f, 1), self.mask(OBL_A, f, 1), self.mask(OBL_B, f, 1)], cap),
            1 => {
                let mut sols = s5.phase_a(self.mask(X_OUT, f, 1), self.mask(OBL_A, f, 1), 0, cap, None);
                sols.truncate(cap);
                sols
            }
            2 => {
                let rank = |o: usize| self.ranks.rank16[self.mask(o, f, 0) as usize] as usize;
                self.step3(&Node3 { x: rank(X_IN), odd: self.wings_odd(f, [0, 2, 3, 5]), a: rank(OBL_A), b: rank(OBL_B) }, cap)
            }
            _ => vec![s5.phase_b(self.mask(X_OUT, f, 0), self.mask(OBL_A, f, 0), self.wings_odd(f, [0, 1, 4, 5]))],
        }
    }

    #[inline(always)]
    fn h1(&self, n: &[u32; 3]) -> usize {
        self.in_rl[n[0] as usize].max(self.unpaired[(n[1] ^ n[2]) as usize]) as usize
    }

    #[inline(always)]
    fn h3(&self, n: &Node3) -> usize {
        self.in_ud[n.x * 2 + n.odd as usize].max(self.obl_ud[n.a * N_UD + n.b]) as usize
    }

    /// Step 3: the shortest ways (up to `cap`).
    fn step3(&self, root: &Node3, cap: usize) -> Vec<Vec<u8>> {
        let h = self.h3(root);
        if h == 0 {
            return vec![Vec::new()];
        }
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        for depth in h..=h + 14 {
            let found: Vec<Vec<u8>> = (0..self.m3.len())
                .into_par_iter()
                .flat_map_iter(|k| {
                    let mut run = Run3 { s: self, cap, count: &count, stop: &stop, out: Vec::new(), path: [0; 32] };
                    run.dfs(root, k, depth, 0);
                    run.out
                })
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// Step 1: the shortest ways (up to `cap`) to bring the R/L inner x
    /// centres onto R and L and to pair the R/L obliques.
    fn step1(&self, root: [u32; 3], cap: usize) -> Vec<Vec<u8>> {
        let h = self.h1(&root);
        if h == 0 {
            return vec![Vec::new()];
        }
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        for depth in h..=h + 14 {
            let found: Vec<Vec<u8>> = (0..54u8)
                .into_par_iter()
                .flat_map_iter(|m| {
                    let mut run = Run1 { s: self, cap, count: &count, stop: &stop, out: Vec::new(), path: [0; 32] };
                    run.dfs(&root, m, depth, 0);
                    run.out
                })
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }
}

/// A lower bound on the moves that pair the obliques of one axis, from the
/// set of places where an oblique and its mate differ (one is of the axis,
/// the other is not). Turns of one or two layers move mates together, so
/// they only move that set around. A 3-layer turn brings new mates together
/// at eight places; here it is allowed to leave those eight in any state,
/// which can only make the count too small.
fn gen_unpaired(a: &CentreOrbit, b: &CentreOrbit, perm: &BitPerm) -> Vec<u8> {
    // Per 3-layer move: the eight places, and where the other sixteen come from.
    let deep: Vec<(u32, [u8; 24])> = (36..54)
        .map(|m| {
            let touched = (0..24).filter(|&i| a.src[m][i] != b.src[m][i]).fold(0u32, |x, i| x | 1 << i);
            assert_eq!(touched.count_ones(), 8);
            (touched, a.src[m])
        })
        .collect();
    let dist: Vec<AtomicU8> = (0..1usize << 24).into_par_iter().map(|_| AtomicU8::new(FAR)).collect();
    dist[0].store(0, Relaxed);
    let set = |x: usize, v: u8| dist[x].load(Relaxed) == FAR && dist[x].compare_exchange(FAR, v, Relaxed, Relaxed).is_ok();
    for depth in 0..20u8 {
        let n: u64 = (0..1u32 << 24)
            .into_par_iter()
            .with_min_len(1 << 12)
            .map(|d| {
                if dist[d as usize].load(Relaxed) != depth {
                    return 0;
                }
                let mut new = 0;
                for m in 0..36u8 {
                    new += set(perm.apply(m, d) as usize, depth + 1) as u64;
                }
                for (touched, src) in &deep {
                    let base = (0..24).filter(|&i| (touched >> i) & 1 == 0).fold(0u32, |x, i| x | ((d >> src[i]) & 1) << i);
                    // Every subset of the eight places.
                    let mut sub = *touched;
                    loop {
                        new += set((base | sub) as usize, depth + 1) as u64;
                        if sub == 0 {
                            break;
                        }
                        sub = (sub - 1) & touched;
                    }
                }
                new
            })
            .sum();
        eprintln!("  6x6x6 pairing bound: depth {:2}  {:>10} new", depth + 1, n);
        if n == 0 {
            break;
        }
    }
    let mut v = std::mem::ManuallyDrop::new(dist);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

/// 3-layer quarter turns move one inner slice a quarter turn: an odd permutation of the inner wings.
fn flips_inner(m: u8) -> bool {
    m >= 36 && m % 3 != 1
}

/// Step 3 state: inner x centres, inner wing parity, the two oblique orbits.
#[derive(Clone, Copy)]
struct Node3 {
    x: usize,
    odd: bool,
    a: usize,
    b: usize,
}

struct Run3<'a> {
    s: &'a Stage6,
    cap: usize,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    out: Vec<Vec<u8>>,
    path: [u8; 32],
}

impl Run3<'_> {
    fn dfs(&mut self, n: &Node3, k: usize, rem: usize, depth: usize) -> bool {
        let s = self.s;
        let m = s.m3[k];
        let next = Node3 {
            x: s.in_ud_mv[n.x][k] as usize,
            odd: n.odd ^ flips_inner(m),
            a: s.obl_ud_mv[0][n.a][k] as usize,
            b: s.obl_ud_mv[1][n.b][k] as usize,
        };
        let h = s.h3(&next);
        if h > rem - 1 {
            return false;
        }
        self.path[depth] = m;
        if rem == 1 {
            if h == 0 {
                self.out.push(self.path[..=depth].to_vec());
                if self.count.fetch_add(1, Relaxed) + 1 >= self.cap {
                    self.stop.store(true, Relaxed);
                    return true;
                }
            }
            return false;
        }
        if h == 0 {
            return false;
        }
        if self.stop.load(Relaxed) {
            return true;
        }
        for k2 in 0..s.m3.len() {
            if !skip54(m, s.m3[k2]) && self.dfs(&next, k2, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}

struct Run1<'a> {
    s: &'a Stage6,
    cap: usize,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    out: Vec<Vec<u8>>,
    path: [u8; 32],
}

impl Run1<'_> {
    fn dfs(&mut self, n: &[u32; 3], m: u8, rem: usize, depth: usize) -> bool {
        let s = self.s;
        let next = [s.perms[X_IN].apply(m, n[0]), s.perms[OBL_A].apply(m, n[1]), s.perms[OBL_B].apply(m, n[2])];
        let h = s.h1(&next);
        if h > rem - 1 {
            return false;
        }
        let done = next[0] == GOAL_RL && next[1] == next[2];
        self.path[depth] = m;
        if rem == 1 {
            if done {
                self.out.push(self.path[..=depth].to_vec());
                if self.count.fetch_add(1, Relaxed) + 1 >= self.cap {
                    self.stop.store(true, Relaxed);
                    return true;
                }
            }
            return false;
        }
        if done {
            return false;
        }
        if self.stop.load(Relaxed) {
            return true;
        }
        for m2 in 0..54u8 {
            if !skip54(m, m2) && self.dfs(&next, m2, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}
