//! 7x7x7, first part: every centre onto a face of its own axis, and both
//! wing orbits into even permutations.
//!
//! The six centre orbits are the inner x and inner t centres (the centres of
//! the 5x5x5 formed by layers 1, 3, 4, 5 and 7), the outer x centres, the
//! outer t centres and two mirror orbits of obliques. In the row next to an
//! edge of a face an oblique, an outer t centre and the other oblique lie
//! side by side: a *triple*. Turns of one or two layers move a triple as one
//! piece, the way the t centre of a 5x5x5 moves; only 3-layer turns take
//! triples apart. A triple is *formed* for an axis when its three centres
//! are all of that axis or none is. Five steps, each keeping what the ones
//! before achieved:
//!
//! 1. R/L inner x and inner t centres onto R and L: phase A of the 5x5x5,
//!    on the inner cube (its wide turn is our 3-layer turn);
//! 2. R/L triples formed. Only the 3-layer turns that keep step 1 (any turn
//!    of R and L, half turns of the rest) change triples; turns of one and
//!    two layers set them up;
//! 3. R/L outer x centres and triples onto R and L with 1- and 2-layer
//!    turns: phase A of the 5x5x5 again, on the outer cube;
//! 4. U/D inner x and inner t centres onto U and D, inner wings even, U/D
//!    triples formed, in one search bounded by the exact table of phase B
//!    and exact tables for each two of the three orbits of a triple;
//! 5. U/D outer x centres and triples onto U and D, outer wings even:
//!    phase B of the 5x5x5 on the outer cube.

use crate::cube::Cube;
use crate::cuben::Geometry;
use crate::orbits::{centre_orbits, CentreOrbit};
use crate::stage5::Stage5;
use crate::stage6::skip54;
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes, Ranks};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering::Relaxed};

const GOAL_RL: u32 = 0xFF_0000;
const GOAL_UD: u32 = 0x00_00FF;
const N_UD: usize = 12870;
const FAR: u8 = 255;
const UNSEEN: u8 = 15;

/// Orbit indices, in the order `centre_orbits` lists them for a 7x7x7.
pub const X_OUT: usize = 0;
pub const OBL_A: usize = 1;
pub const T_OUT: usize = 2;
pub const OBL_B: usize = 3;
pub const X_IN: usize = 4;
pub const T_IN: usize = 5;

/// Layers of the inner and of the outer 5x5x5, and of the 4x4x4 that shows
/// the inner or the outer wings.
pub const INNER5: [usize; 5] = [0, 2, 3, 4, 6];
pub const OUTER5: [usize; 5] = [0, 1, 3, 5, 6];
pub const INNER_WINGS: [usize; 4] = [0, 2, 4, 6];
pub const OUTER_WINGS: [usize; 4] = [0, 1, 5, 6];

/// Our move for a move of the inner 5x5x5: its wide turn is our 3-layer turn.
#[inline(always)]
pub fn from_inner(m: u8) -> u8 {
    if m < 18 {
        m
    } else {
        m + 18
    }
}

/// Move numbers: `face * 3 + power` for faces U R F D L B, plus 18 per extra layer.
pub fn moves_where(keep: impl Fn(usize, usize, usize) -> bool) -> Vec<u8> {
    (0..54u8).filter(|&m| keep(m as usize / 3 % 6, m as usize / 18 + 1, m as usize % 3)).collect()
}

/// 3-layer quarter turns move one inner slice a quarter turn: an odd permutation of the inner wings.
#[inline(always)]
pub fn flips_inner(m: u8) -> bool {
    m >= 36 && m % 3 != 1
}

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

#[inline(always)]
pub fn nibble(bytes: &[u8], idx: usize) -> u8 {
    (bytes[idx >> 1] >> ((idx & 1) * 4)) & 15
}

fn into_bytes(v: Vec<AtomicU8>) -> Vec<u8> {
    let mut v = std::mem::ManuallyDrop::new(v);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

/// A problem for `ida`: a state, the moves allowed, and a lower bound.
pub trait Problem: Sync {
    type Node: Copy + Send + Sync;
    fn moves(&self) -> &[u8];
    /// The state after the `k`-th of `moves`.
    fn next(&self, n: &Self::Node, k: usize) -> Self::Node;
    /// Never more than the moves still needed.
    fn bound(&self, n: &Self::Node) -> usize;
    /// Asked only where the bound is 0.
    fn done(&self, n: &Self::Node) -> bool;
    /// False for a move that cannot be the last of a shortest solution.
    fn may_end(&self, _k: usize) -> bool {
        true
    }
}

struct Dfs<'a, P: Problem> {
    p: &'a P,
    cap: usize,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    nodes: &'a AtomicU64,
    budget: u64,
    seen: u64,
    out: Vec<Vec<u8>>,
    path: [u8; 48],
}

impl<P: Problem> Dfs<'_, P> {
    fn go(&mut self, n: &P::Node, k: usize, rem: usize, depth: usize) -> bool {
        let p = self.p;
        if rem == 1 && !p.may_end(k) {
            return false;
        }
        let next = p.next(n, k);
        let h = p.bound(&next);
        if h > rem - 1 {
            return false;
        }
        let m = p.moves()[k];
        self.path[depth] = m;
        let done = h == 0 && p.done(&next);
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
        // A solution that passes through the goal is a shorter one with moves added.
        if done {
            return false;
        }
        self.seen += 1;
        if self.seen & 0x3FFF == 0 {
            if self.nodes.fetch_add(0x4000, Relaxed) + 0x4000 > self.budget {
                self.stop.store(true, Relaxed);
            }
            if self.stop.load(Relaxed) {
                return true;
            }
        }
        for k2 in 0..p.moves().len() {
            if !skip54(m, p.moves()[k2]) && self.go(&next, k2, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}

/// Iterative deepening: the solutions of the shortest length and of the
/// next `extra` lengths, until there are `cap` of them. Empty if more than
/// `budget` positions were visited before the first solution.
pub fn ida<P: Problem>(p: &P, root: &P::Node, extra: usize, cap: usize, budget: u64) -> Vec<Vec<u8>> {
    let h = p.bound(root);
    if h == 0 && p.done(root) {
        return vec![Vec::new()];
    }
    let nodes = AtomicU64::new(0);
    let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
    let mut all: Vec<Vec<u8>> = Vec::new();
    let mut optimum = None;
    for depth in h.max(1)..=h + 24 {
        if optimum.is_some_and(|o| depth > o + extra) || stop.load(Relaxed) {
            break;
        }
        if std::env::var_os("CUBE7_TRACE").is_some() {
            eprintln!("      depth {:2} (bound {}), {} positions so far", depth, h, nodes.load(Relaxed));
        }
        let found: Vec<Vec<u8>> = (0..p.moves().len())
            .into_par_iter()
            .with_max_len(1)
            .flat_map_iter(|k| {
                let mut dfs = Dfs { p, cap, count: &count, stop: &stop, nodes: &nodes, budget, seen: 0, out: Vec::new(), path: [0; 48] };
                if !stop.load(Relaxed) {
                    dfs.go(root, k, depth, 0);
                }
                dfs.out
            })
            .collect();
        if !found.is_empty() && optimum.is_none() {
            optimum = Some(depth);
        }
        all.extend(found);
    }
    all
}

/// Beam search, for problems whose bound is too weak for `ida`: level by
/// level, only the `width` states with the lowest `score` are continued
/// (states reached twice count once). Returns up to `cap` solutions of the
/// first length at which any is found; they need not be the shortest that exist.
pub fn beam<P: Problem>(
    p: &P,
    root: &P::Node,
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
) -> Vec<Vec<u8>>
where
    P::Node: Ord,
{
    if p.bound(root) == 0 && p.done(root) {
        return vec![Vec::new()];
    }
    // Per level: each kept state's parent in the level before and the move that led to it.
    let mut trail: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut level: Vec<(P::Node, u8)> = vec![(*root, FAR)];
    for _ in 0..max_len {
        // (score, state, parent, index of the move); a score of 0 marks a solution.
        let mut next: Vec<(u32, P::Node, u32, u8)> = level
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, (n, last))| {
                let moves = p.moves();
                let score = &score;
                (0..moves.len()).filter(move |&k| *last == FAR || !skip54(*last, moves[k])).map(move |k| {
                    let c = p.next(n, k);
                    let solved = p.bound(&c) == 0 && p.done(&c);
                    (if solved { 0 } else { 1 + score(&c) }, c, i as u32, k as u8)
                })
            })
            .collect();
        next.par_sort_unstable();
        next.dedup_by(|x, y| x.1 == y.1);
        let solved = next.iter().take_while(|x| x.0 == 0).count();
        if solved > 0 {
            return next[..solved.min(cap)]
                .iter()
                .map(|x| {
                    let mut path = vec![p.moves()[x.3 as usize]];
                    let mut at = x.2 as usize;
                    for step in trail.iter().rev() {
                        path.push(step[at].1);
                        at = step[at].0 as usize;
                    }
                    path.reverse();
                    path
                })
                .collect();
        }
        next.truncate(width);
        trail.push(next.iter().map(|x| (x.2, p.moves()[x.3 as usize])).collect());
        level = next.iter().map(|x| (x.1, p.moves()[x.3 as usize])).collect();
    }
    Vec::new()
}

pub struct Stage7 {
    pub g: Geometry,
    pub orbits: Vec<CentreOrbit>,
    pub perms: Vec<BitPerm>,
    pub ranks: Ranks,
    /// Step 2: its moves, and for each two of the three orbits of a triple
    /// (the obliques, first oblique and t, t and second oblique) a lower
    /// bound on the moves that make them agree, by the mask of places where
    /// they differ.
    m2: Vec<u8>,
    diff2: [Bytes; 3],
    /// Step 4: its moves; where they take the U/D pattern (one of 12,870) of
    /// the inner x, inner t, first oblique, outer t and second oblique
    /// orbits; and the exact moves to make each two orbits of a triple agree.
    m4: Vec<u8>,
    mv4: [Vec<u16>; 5],
    pair4: [Bytes; 3],
}

/// A staging: the moves and how many each step took.
#[derive(Clone)]
pub struct Staged7 {
    pub moves: Vec<u8>,
    pub lens: [usize; 5],
}

/// A lower bound on the moves that make two orbits of a triple agree on one
/// axis, from the set of places where one centre is of the axis and its
/// neighbour in the other orbit is not. Turns that move the two orbits alike
/// only move that set around. Where a turn moves them differently it brings
/// new neighbours together; here it may leave those places in any state,
/// which can only make the count too small.
fn gen_diff(p: &CentreOrbit, q: &CentreOrbit, perm: &BitPerm, moves: &[u8]) -> Vec<u8> {
    let acts: Vec<(u8, u32, [u8; 24])> = moves
        .iter()
        .map(|&m| {
            let (a, b) = (&p.src[m as usize], &q.src[m as usize]);
            let touched = (0..24).filter(|&i| a[i] != b[i]).fold(0u32, |x, i| x | 1 << i);
            (m, touched, *a)
        })
        .collect();
    let dist: Vec<AtomicU8> = (0..1usize << 24).into_par_iter().map(|_| AtomicU8::new(FAR)).collect();
    dist[0].store(0, Relaxed);
    let set = |x: usize, v: u8| dist[x].load(Relaxed) == FAR && dist[x].compare_exchange(FAR, v, Relaxed, Relaxed).is_ok();
    for depth in 0..30u8 {
        let n: u64 = (0..1u32 << 24)
            .into_par_iter()
            .with_min_len(1 << 12)
            .map(|d| {
                if dist[d as usize].load(Relaxed) != depth {
                    return 0;
                }
                let mut new = 0;
                for (m, touched, src) in &acts {
                    if *touched == 0 {
                        new += set(perm.apply(*m, d) as usize, depth + 1) as u64;
                        continue;
                    }
                    let base = (0..24).filter(|&i| (touched >> i) & 1 == 0).fold(0u32, |x, i| x | ((d >> src[i]) & 1) << i);
                    // Every subset of the places where new neighbours meet.
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
        eprintln!("  7x7x7 triple bound: depth {:2}  {:>10} new", depth + 1, n);
        if n == 0 {
            break;
        }
    }
    into_bytes(dist)
}

/// Exact moves until two orbits show the same U/D pattern, for every two
/// patterns, four bits each. `p[r * moves + k]` is pattern `r` after move `k`.
fn gen_pair(p: &[u16], q: &[u16], moves: usize) -> Vec<u8> {
    let dist: Vec<AtomicU8> = (0..N_UD * N_UD / 2).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    for a in 0..N_UD {
        nib_set(&dist, a * N_UD + a, 0);
    }
    let mut total = N_UD as u64;
    for depth in 0..UNSEEN - 1 {
        let n: u64 = (0..N_UD)
            .into_par_iter()
            .map(|a| {
                let mut new = 0;
                for b in 0..N_UD {
                    if nib(&dist, a * N_UD + b) != depth {
                        continue;
                    }
                    for k in 0..moves {
                        let j = p[a * moves + k] as usize * N_UD + q[b * moves + k] as usize;
                        new += nib_set(&dist, j, depth + 1) as u64;
                    }
                }
                new
            })
            .sum();
        total += n;
        eprintln!("  7x7x7 step 4 triple table: depth {:2}  {:>10} new", depth + 1, n);
        if n == 0 {
            break;
        }
    }
    assert_eq!(total as usize, N_UD * N_UD, "every two patterns can be made to agree within 14 moves");
    into_bytes(dist)
}

impl Stage7 {
    pub fn load() -> Stage7 {
        let g = Geometry::new(7);
        let orbits = centre_orbits(&g);
        assert_eq!(orbits.len(), 6);
        // The three orbits of a triple move alike under turns of one and two layers.
        for m in 0..36 {
            assert!(orbits[OBL_A].src[m] == orbits[T_OUT].src[m] && orbits[OBL_B].src[m] == orbits[T_OUT].src[m]);
        }
        let perms: Vec<BitPerm> = orbits.iter().map(|o| BitPerm::from_src(&o.src)).collect();
        let ranks = Ranks::new();
        let dir = table_dir();
        let side = |f: usize| f != 1 && f != 4;

        // Step 2: everything that keeps the inner R/L centres on R and L.
        let m2 = moves_where(|f, layers, power| layers < 3 || !side(f) || power == 1);
        let diff = |name: &str, p: usize, q: usize| {
            load_or_gen(&dir.join(name), 1 << 24, || gen_diff(&orbits[p], &orbits[q], &perms[p], &m2))
        };
        let diff2 = [
            diff("stage777-2-ab.bin", OBL_A, OBL_B),
            diff("stage777-2-at.bin", OBL_A, T_OUT),
            diff("stage777-2-tb.bin", T_OUT, OBL_B),
        ];

        // Step 4: U F D B face turns, turns of R and L of two or three layers,
        // half turns of two or three layers of the rest.
        let m4 = moves_where(|f, layers, power| match layers {
            1 => side(f),
            _ => !side(f) || power == 1,
        });
        let table = |o: usize| -> Vec<u16> {
            let mut out = Vec::with_capacity(N_UD * m4.len());
            for r in 0..N_UD {
                for &m in &m4 {
                    let y = perms[o].apply(m, ranks.unrank16[r] as u32);
                    assert!(y < 1 << 16, "these moves keep the R/L centres in place");
                    out.push(ranks.rank16[y as usize]);
                }
            }
            out
        };
        let mv4 = [table(X_IN), table(T_IN), table(OBL_A), table(T_OUT), table(OBL_B)];
        let pair = |name: &str, p: usize, q: usize| {
            load_or_gen(&dir.join(name), N_UD * N_UD / 2, || gen_pair(&mv4[p], &mv4[q], m4.len()))
        };
        let pair4 = [pair("stage777-4-ab.bin", 2, 4), pair("stage777-4-at.bin", 2, 3), pair("stage777-4-tb.bin", 3, 4)];
        Stage7 { g, orbits, perms, ranks, m2, diff2, m4, mv4, pair4 }
    }

    /// Positions of the centres of one axis colour pair (0 U/D, 1 R/L, 2 F/B) in an orbit.
    pub fn mask(&self, orbit: usize, facelets: &[u8], axis: u8) -> u32 {
        let col = self.orbits[orbit].colours(facelets);
        (0..24).fold(0, |m, i| m | ((col[i] % 3 == axis) as u32) << i)
    }

    /// Positions where an orbit shows the first colour of an axis (U, R or F).
    pub fn primary(&self, orbit: usize, facelets: &[u8]) -> u32 {
        let col = self.orbits[orbit].colours(facelets);
        (0..24).fold(0, |m, i| m | ((col[i] < 3) as u32) << i)
    }

    /// Parity of the permutation of the wings in the given layers.
    pub fn wings_odd(&self, facelets: &[u8], layers: [usize; 4]) -> bool {
        let sub = self.g.to_string(&self.g.sub_facelets(facelets, &layers));
        Cube::from_facelets(&sub).expect("four layers of a 7x7x7 form a 4x4x4").wing_parity() == 1
    }

    pub fn is_staged(&self, facelets: &[u8]) -> bool {
        (0..6).all(|o| self.mask(o, facelets, 1) == GOAL_RL && self.mask(o, facelets, 0) == GOAL_UD)
            && !self.wings_odd(facelets, INNER_WINGS)
            && !self.wings_odd(facelets, OUTER_WINGS)
    }

    /// The best stagings found, shortest first. `width` candidates are
    /// carried from step to step, each continued in up to `width` ways;
    /// `beam` is the width of the beam search of step 2.
    pub fn stage(&self, s5: &Stage5, facelets: &[u8], width: usize, beam: usize) -> Vec<Staged7> {
        let start = Staged7 { moves: Vec::new(), lens: [0; 5] };
        let mut cands = vec![(start, facelets.to_vec())];
        for step in 0..5 {
            let mut next: Vec<(Staged7, Vec<u8>)> = cands
                .par_iter()
                .flat_map_iter(|(c, f)| {
                    let sols = self.step(s5, step, f, width, beam);
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

    /// Development aid: how long step 2 is after each of many ways to do step 1.
    pub fn probe_step2(&self, s5: &Stage5, facelets: &[u8], extra: usize, cands: usize, budget: u64) {
        let mut firsts = s5.phase_a(self.mask(X_IN, facelets, 1), self.mask(T_IN, facelets, 1), extra, cands, None);
        firsts.sort_by_key(|s| s.len());
        firsts.truncate(cands);
        let rows: Vec<(usize, usize, usize, usize)> = firsts
            .par_iter()
            .map(|sol| {
                let mut f = facelets.to_vec();
                self.g.apply_seq(&mut f, &sol.iter().map(|&m| from_inner(m) as usize).collect::<Vec<_>>());
                let root = [self.mask(OBL_A, &f, 1), self.mask(T_OUT, &f, 1), self.mask(OBL_B, &f, 1)];
                let p = Step2 { s: self };
                let bad = (0..24).filter(|&i| (root[0] ^ root[1] | root[1] ^ root[2]) >> i & 1 == 1).count();
                let found = self.step2(&root, budget as usize, 1);
                (sol.len(), p.bound(&root), bad, found.first().map_or(99, |x| x.len()))
            })
            .collect();
        for r in &rows {
            eprintln!("    step 1 {:2} moves, step 2 bound {}, {:2} triples not formed, step 2 {:2} moves", r.0, r.1, r.2, r.3);
        }
    }

    /// Step 2 by beam search. States are ranked by the two bounds for the
    /// outer t centres against each oblique orbit, then by how many obliques
    /// do not agree with the outer t centre beside them.
    fn step2(&self, root: &[u32; 3], width: usize, cap: usize) -> Vec<Vec<u8>> {
        let d = &self.diff2;
        let score = |n: &[u32; 3]| {
            let (x, y) = (n[0] ^ n[1], n[1] ^ n[2]);
            (d[1][x as usize] as u32 + d[2][y as usize] as u32) * 64 + x.count_ones() + y.count_ones()
        };
        beam(&Step2 { s: self }, root, score, width, cap, 40)
    }

    /// Solutions of one step from a cube on which the earlier steps are done.
    fn step(&self, s5: &Stage5, step: usize, f: &[u8], cap: usize, beam: usize) -> Vec<Vec<u8>> {
        if std::env::var_os("CUBE7_TRACE").is_some() {
            eprintln!("    staging step {}", step + 1);
        }
        let mut sols = match step {
            0 => s5
                .phase_a(self.mask(X_IN, f, 1), self.mask(T_IN, f, 1), 0, cap, None)
                .into_iter()
                .map(|sol| sol.into_iter().map(from_inner).collect())
                .collect(),
            1 => self.step2(&[self.mask(OBL_A, f, 1), self.mask(T_OUT, f, 1), self.mask(OBL_B, f, 1)], beam, cap),
            2 => s5.phase_a(self.mask(X_OUT, f, 1), self.mask(T_OUT, f, 1), 0, cap, None),
            3 => {
                let rank = |o: usize| self.ranks.rank16[self.mask(o, f, 0) as usize];
                let root = Node4 {
                    x: rank(X_IN),
                    t: rank(T_IN),
                    odd: self.wings_odd(f, INNER_WINGS),
                    a: rank(OBL_A),
                    o: rank(T_OUT),
                    b: rank(OBL_B),
                };
                ida(&Step4 { s: self, s5 }, &root, 0, cap, u64::MAX)
            }
            _ => vec![s5.phase_b(self.mask(X_OUT, f, 0), self.mask(T_OUT, f, 0), self.wings_odd(f, OUTER_WINGS))],
        };
        sols.truncate(cap);
        sols
    }
}

/// Step 2: the R/L masks of the first oblique, outer t and second oblique orbits.
struct Step2<'a> {
    s: &'a Stage7,
}

impl Problem for Step2<'_> {
    type Node = [u32; 3];

    fn moves(&self) -> &[u8] {
        &self.s.m2
    }

    #[inline(always)]
    fn next(&self, n: &[u32; 3], k: usize) -> [u32; 3] {
        let (m, p) = (self.s.m2[k], &self.s.perms);
        [p[OBL_A].apply(m, n[0]), p[T_OUT].apply(m, n[1]), p[OBL_B].apply(m, n[2])]
    }

    #[inline(always)]
    fn bound(&self, n: &[u32; 3]) -> usize {
        let d = &self.s.diff2;
        d[0][(n[0] ^ n[2]) as usize].max(d[1][(n[0] ^ n[1]) as usize]).max(d[2][(n[1] ^ n[2]) as usize]) as usize
    }

    fn done(&self, n: &[u32; 3]) -> bool {
        n[0] == n[1] && n[1] == n[2]
    }

    /// Only a 3-layer turn changes a triple.
    fn may_end(&self, k: usize) -> bool {
        self.s.m2[k] >= 36
    }
}

/// Step 4: the U/D patterns of the inner x and inner t centres, the parity
/// of the inner wings, and the U/D patterns of the three orbits of the triples.
#[derive(Clone, Copy)]
struct Node4 {
    x: u16,
    t: u16,
    odd: bool,
    a: u16,
    o: u16,
    b: u16,
}

struct Step4<'a> {
    s: &'a Stage7,
    s5: &'a Stage5,
}

impl Problem for Step4<'_> {
    type Node = Node4;

    fn moves(&self) -> &[u8] {
        &self.s.m4
    }

    #[inline(always)]
    fn next(&self, n: &Node4, k: usize) -> Node4 {
        let (mv, w) = (&self.s.mv4, self.s.m4.len());
        Node4 {
            x: mv[0][n.x as usize * w + k],
            t: mv[1][n.t as usize * w + k],
            odd: n.odd ^ flips_inner(self.s.m4[k]),
            a: mv[2][n.a as usize * w + k],
            o: mv[3][n.o as usize * w + k],
            b: mv[4][n.b as usize * w + k],
        }
    }

    #[inline(always)]
    fn bound(&self, n: &Node4) -> usize {
        let (a, o, b) = (n.a as usize, n.o as usize, n.b as usize);
        let p = &self.s.pair4;
        self.s5
            .b_dist(n.x as usize, n.t as usize, n.odd)
            .max(nibble(&p[0], a * N_UD + b))
            .max(nibble(&p[1], a * N_UD + o))
            .max(nibble(&p[2], o * N_UD + b)) as usize
    }

    /// The tables are exact, so a bound of 0 is the goal.
    fn done(&self, _: &Node4) -> bool {
        true
    }

    /// Only a 3-layer turn moves an inner centre off its face or changes a triple.
    fn may_end(&self, k: usize) -> bool {
        self.s.m4[k] >= 36
    }
}
