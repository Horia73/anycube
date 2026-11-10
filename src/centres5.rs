//! 5x5x5 centres in one step: every centre onto a face of its own axis,
//! with all 36 moves, as short as a search can make it.
//!
//! Each of the two orbits (x and t centres) has an exact table: the number
//! of moves that stage that orbit alone, for every one of its 9.5 billion
//! colourings, stored once per class under the sixteen symmetries that keep
//! the R/L axis. The search is bounded by the larger of the two.
//!
//! That bound is about 9 where the answer is about 15, so the search is exact
//! but takes minutes per cube (340 s for the one cube measured, which the
//! two-phase staging does in 17 moves). It is not part of any mode; `--joint`
//! turns it on. Dropping branches by the sum of the two distances was tried
//! and found nothing shorter than the two-phase staging.

use crate::c5::{tables5, Cube5};
use crate::cube::{skip_move, tables as cube_tables};
use crate::sym::{axis_move_maps, closure, conjugators, destinations, renamed, CombRank, MaskMap};
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes, Ranks};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

const ALL: u32 = 0xFF_FFFF;
const RL_SET: u32 = 0xFF_0000;
const UD_SET: u32 = 0x00_00FF;
const FB_SET: u32 = 0x00_FF00;
const N_UD: usize = 12870;
const UNSEEN: u8 = 15;

#[inline(always)]
fn nib(bytes: &[AtomicU8], idx: usize) -> u8 {
    (bytes[idx >> 1].load(Relaxed) >> ((idx & 1) * 4)) & 15
}

/// Sets an unseen entry to `v`; returns true if this call did it.
#[inline(always)]
fn nib_set(bytes: &[AtomicU8], idx: usize, v: u8) -> bool {
    let shift = (idx & 1) * 4;
    let cell = &bytes[idx >> 1];
    let mut cur = cell.load(Relaxed);
    loop {
        if (cur >> shift) & 15 != UNSEEN {
            return false;
        }
        match cell.compare_exchange_weak(cur, (cur & !(15 << shift)) | v << shift, Relaxed, Relaxed) {
            Ok(_) => return true,
            Err(now) => cur = now,
        }
    }
}

/// The bits of `mask` at the positions set in `free`, packed together.
#[inline(always)]
fn compress(mut mask: u32, free: u32) -> usize {
    let mut out = 0;
    while mask != 0 {
        let bit = mask & mask.wrapping_neg();
        out |= 1 << (free & (bit - 1)).count_ones();
        mask ^= bit;
    }
    out
}

/// The inverse of `compress`.
fn deposit(word: usize, mut free: u32) -> u32 {
    let (mut out, mut i) = (0, 0);
    while free != 0 {
        let bit = free & free.wrapping_neg();
        if (word >> i) & 1 == 1 {
            out |= bit;
        }
        free ^= bit;
        i += 1;
    }
    out
}

/// A symmetry of one orbit: where positions go, and whether the U/D and
/// F/B axes change places.
struct OrbitSym {
    map: MaskMap,
    swap: bool,
}

impl OrbitSym {
    /// The U/D positions of a colouring after the symmetry.
    #[inline(always)]
    fn ud(&self, rl: u32, ud: u32) -> u32 {
        self.map.apply(if self.swap { !(rl | ud) & ALL } else { ud })
    }
}

/// One orbit of 24 centres: positions of the R/L-coloured and of the
/// U/D-coloured ones as bit masks.
pub struct Orbit {
    pub bp: BitPerm,
    syms: Vec<OrbitSym>,
    /// Per R/L mask, by rank: `class << 6 | symmetry` taking it to the class representative.
    cls: Vec<u32>,
    reps: Vec<u32>,
    dist: Bytes,
}

impl Orbit {
    fn load(name: &str, src: &[[u8; 24]], rank: &CombRank, ranks: &Ranks) -> Orbit {
        let bp = BitPerm::from_src(src);
        let dest = destinations(src);
        let moves: Vec<u8> = (0..36).collect();
        let mut generators = Vec::new();
        for mu in axis_move_maps() {
            let act = |m: usize, x: usize| dest[m][x] as usize;
            for p in conjugators(24, &renamed(&moves, &mu), &act, &act, &[]) {
                let map = MaskMap::new(&p);
                if map.apply(RL_SET) != RL_SET {
                    continue;
                }
                let swap = match map.apply(UD_SET) {
                    UD_SET => vec![0, 1],
                    FB_SET => vec![1, 0],
                    _ => continue,
                };
                generators.push(vec![p, swap]);
            }
        }
        let syms: Vec<OrbitSym> =
            closure(&generators).into_iter().map(|e| OrbitSym { map: MaskMap::new(&e[0]), swap: e[1][0] == 1 }).collect();
        let n_rl = rank.unrank.len();
        let dir = table_dir();
        let cls = load_or_gen(&dir.join(format!("centres555-{}-classes.bin", name)), n_rl * 4, || {
            let best: Vec<(u32, u32)> = (0..n_rl)
                .into_par_iter()
                .map(|r| syms.iter().enumerate().map(|(s, sym)| (sym.map.apply(rank.unrank[r]), s as u32)).min().unwrap())
                .collect();
            let mut reps: Vec<u32> = best.iter().map(|b| b.0).collect();
            reps.sort_unstable();
            reps.dedup();
            best.iter().flat_map(|&(rep, s)| ((reps.binary_search(&rep).unwrap() as u32) << 6 | s).to_le_bytes()).collect()
        });
        let cls: Vec<u32> = cls.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect();
        let n_cls = cls.iter().map(|&c| c >> 6).max().unwrap() as usize + 1;
        let mut reps = vec![0u32; n_cls];
        for (r, &c) in cls.iter().enumerate() {
            reps[(c >> 6) as usize] = syms[(c & 63) as usize].map.apply(rank.unrank[r]);
        }
        let dist = load_or_gen(&dir.join(format!("centres555-{}.bin", name)), (n_cls * N_UD).div_ceil(2), || {
            eprintln!("  5x5x5 {} centres: {} symmetries, {} classes", name, syms.len(), n_cls);
            gen_orbit(&bp, &syms, &cls, &reps, rank, ranks)
        });
        Orbit { bp, syms, cls, reps, dist }
    }

    /// Moves needed to stage this orbit alone (15 stands for 15 or more).
    #[inline(always)]
    pub fn dist(&self, rank: &CombRank, ranks: &Ranks, rl: u32, ud: u32) -> u8 {
        let cs = self.cls[rank.rank(rl)];
        let c = (cs >> 6) as usize;
        let ud = self.syms[(cs & 63) as usize].ud(rl, ud);
        let idx = c * N_UD + ranks.rank16[compress(ud, !self.reps[c] & ALL)] as usize;
        (self.dist[idx >> 1] >> ((idx & 1) * 4)) & 15
    }
}

fn gen_orbit(bp: &BitPerm, syms: &[OrbitSym], cls: &[u32], reps: &[u32], rank: &CombRank, ranks: &Ranks) -> Vec<u8> {
    let n_cls = reps.len();
    let stab: Vec<u64> = reps
        .par_iter()
        .map(|&x| syms.iter().enumerate().filter(|(_, s)| s.map.apply(x) == x).fold(0u64, |a, (i, _)| a | 1 << i))
        .collect();
    let bytes: Vec<AtomicU8> = (0..(n_cls * N_UD).div_ceil(2)).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    // Sets the colouring (representative of class `c`, U/D positions `ud`) and
    // its images under the symmetries that keep the representative.
    let set = |c: usize, ud: u32, v: u8| -> u64 {
        let (rl, mut n, mut st) = (reps[c], 0, stab[c]);
        while st != 0 {
            let s = st.trailing_zeros() as usize;
            st &= st - 1;
            let b = ranks.rank16[compress(syms[s].ud(rl, ud), !rl & ALL)] as usize;
            n += nib_set(&bytes, c * N_UD + b, v) as u64;
        }
        n
    };
    let goal = cls[rank.rank(RL_SET)];
    let mut done = set((goal >> 6) as usize, syms[(goal & 63) as usize].ud(RL_SET, UD_SET), 0);
    for depth in 0..14u8 {
        let n: u64 = (0..n_cls)
            .into_par_iter()
            .with_min_len(4)
            .map(|c| {
                let rl = reps[c];
                // Where each move takes the R/L centres does not depend on the rest.
                let to: Vec<(u32, usize, usize)> = (0..36u8)
                    .map(|m| {
                        let rl2 = bp.apply(m, rl);
                        let cs = cls[rank.rank(rl2)];
                        (rl2, (cs >> 6) as usize, (cs & 63) as usize)
                    })
                    .collect();
                let mut new = 0;
                for b in 0..N_UD {
                    if nib(&bytes, c * N_UD + b) != depth {
                        continue;
                    }
                    let ud = deposit(ranks.unrank16[b] as usize, !rl & ALL);
                    for m in 0..36u8 {
                        let (rl2, c2, s) = to[m as usize];
                        new += set(c2, syms[s].ud(rl2, bp.apply(m, ud)), depth + 1);
                    }
                }
                new
            })
            .sum();
        done += n;
        eprintln!("    depth {:2}  {:>11} new  {:>11} total", depth + 1, n, done);
        if n == 0 {
            break;
        }
    }
    eprintln!("    {} of {} entries need 15 moves or more", (n_cls * N_UD) as u64 - done, n_cls * N_UD);
    let mut v = std::mem::ManuallyDrop::new(bytes);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

/// The colourings of both orbits and the parity of the wing permutation.
#[derive(Clone, Copy)]
struct Node {
    xr: u32,
    xu: u32,
    tr: u32,
    tu: u32,
    odd: bool,
}

pub struct Centres5 {
    pub x: Orbit,
    pub t: Orbit,
    rank: CombRank,
    ranks: Ranks,
}

/// How far a search for stagings goes.
pub struct Budget {
    /// Longest staging worth finding.
    pub max_len: usize,
    /// Lengths searched past the shortest one found.
    pub extra: usize,
    /// Stop collecting at this many.
    pub cap: usize,
    /// Give up at this moment, with whatever has been found.
    pub deadline: Option<Instant>,
}

struct Run<'a> {
    c5: &'a Centres5,
    budget: &'a Budget,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    out: Vec<Vec<u8>>,
    path: [u8; 32],
    ticks: u32,
}

fn mask_of(colours: &[u8; 24], axis: u8) -> u32 {
    (0..24).fold(0, |m, i| m | ((colours[i] % 3 == axis) as u32) << i)
}

impl Centres5 {
    pub fn load() -> Centres5 {
        let rank = CombRank::new(8);
        let ranks = Ranks::new();
        let x = Orbit::load("x", &cube_tables().ct_src, &rank, &ranks);
        let t = Orbit::load("t", &tables5().t_src, &rank, &ranks);
        Centres5 { x, t, rank, ranks }
    }

    fn node(c: &Cube5) -> Node {
        Node {
            xr: mask_of(&c.c4.ct, 1),
            xu: mask_of(&c.c4.ct, 0),
            tr: mask_of(&c.t, 1),
            tu: mask_of(&c.t, 0),
            odd: c.c4.wing_parity() == 1,
        }
    }

    /// Lower bound on the moves that stage both orbits: (x alone, t alone).
    pub fn bounds(&self, c: &Cube5) -> (u8, u8) {
        let n = Self::node(c);
        (self.x.dist(&self.rank, &self.ranks, n.xr, n.xu), self.t.dist(&self.rank, &self.ranks, n.tr, n.tu))
    }

    /// Stagings of a cube, shortest first: every one of the shortest length
    /// and of the next `extra` lengths, as far as the budget allows. Empty if
    /// none of at most `max_len` moves was found in time.
    pub fn stage(&self, cube: &Cube5, budget: &Budget) -> Vec<Vec<u8>> {
        let root = Self::node(cube);
        let (hx, ht) = self.bounds(cube);
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        let mut all: Vec<Vec<u8>> = Vec::new();
        let mut first = None;
        for depth in hx.max(ht) as usize..=budget.max_len {
            if first.is_some_and(|f| depth > f + budget.extra) || stop.load(Relaxed) {
                break;
            }
            if depth == 0 {
                if !root.odd {
                    all.push(Vec::new());
                    first = Some(0);
                }
                continue;
            }
            // One job per first move.
            let found: Vec<Vec<u8>> = (0..36u8)
                .into_par_iter()
                .flat_map_iter(|m| {
                    let mut run =
                        Run { c5: self, budget, count: &count, stop: &stop, out: Vec::new(), path: [0; 32], ticks: 0 };
                    run.step(&root, m, depth, 0);
                    run.out
                })
                .collect();
            if !found.is_empty() && first.is_none() {
                first = Some(depth);
            }
            all.extend(found);
        }
        all.sort_by_key(|s| s.len());
        all
    }
}

impl Run<'_> {
    /// Makes move `m` from `n` with `rem` moves left including it; true to stop the search.
    #[inline(always)]
    fn step(&mut self, n: &Node, m: u8, rem: usize, depth: usize) -> bool {
        let c = self.c5;
        let (xr, xu) = (c.x.bp.apply(m, n.xr), c.x.bp.apply(m, n.xu));
        let hx = c.x.dist(&c.rank, &c.ranks, xr, xu) as usize;
        if hx > rem - 1 {
            return false;
        }
        let (tr, tu) = (c.t.bp.apply(m, n.tr), c.t.bp.apply(m, n.tu));
        let ht = c.t.dist(&c.rank, &c.ranks, tr, tu) as usize;
        if ht > rem - 1 {
            return false;
        }
        let next = Node { xr, xu, tr, tu, odd: n.odd ^ (m >= 18 && m % 3 != 1) };
        let staged = hx == 0 && ht == 0 && !next.odd;
        self.path[depth] = m;
        if rem == 1 {
            if staged {
                self.out.push(self.path[..=depth].to_vec());
                if self.count.fetch_add(1, Relaxed) + 1 >= self.budget.cap {
                    self.stop.store(true, Relaxed);
                    return true;
                }
            }
            return false;
        }
        // A cube staged early was a shorter solution already.
        if staged {
            return false;
        }
        self.ticks += 1;
        if self.ticks & 4095 == 0 {
            if self.budget.deadline.is_some_and(|d| Instant::now() > d) {
                self.stop.store(true, Relaxed);
            }
            if self.stop.load(Relaxed) {
                return true;
            }
        }
        for m2 in 0..36u8 {
            if skip_move(m, m2) {
                continue;
            }
            if self.step(&next, m2, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}
