//! Experiment: the 5x5x5 centres staged by a beam search with all 36 moves
//! that follows a learned estimate of the moves left. Not part of any mode.
//!
//! The estimate is a small network (`ml/centres5_net.py`) trained on cubes
//! K random moves away from staged centres to say K. It sees which axis the
//! centre at each of the 48 positions belongs to, the parity of the wings,
//! and the exact distances there are tables for: each orbit staged alone
//! (`centres5`), and for each axis and orbit the eight centres of that axis
//! brought onto its faces (the first centre phase of `stage5`, one orbit).
//!
//! The search keeps, level by level, the cubes the network puts nearest.
//! A cube with one axis in place is also finished by the exact table of the
//! second centre phase, and what cannot end within `slack` moves of the
//! shortest staging found so far is dropped by the exact distances.

use crate::c5::{tables5, Cube5};
use crate::centres5::Centres5;
use crate::cube::{skip_move, tables as cube_tables};
use crate::stage5::Stage5;
use crate::sym::{CombRank, MaskMap};
use crate::tables::{gen_stage, BitPerm, Ranks};
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

const ALL: u32 = 0xFF_FFFF;
/// Where the centres of each axis belong: U/D, R/L, F/B (a colour modulo 3).
const GOAL: [u32; 3] = [0x00_00FF, 0xFF_0000, 0x00_FF00];
/// Inputs of the network.
pub const N_IN: usize = 153;
/// Bytes of a training record: 48 axes, parity, 8 distances, K.
pub const RECORD: usize = 58;
/// Cubes the network scores in one call.
const CHUNK: usize = 8192;

/// The centres as far as staging goes: where the R/L and the U/D centres of
/// each orbit are, and the parity of the wing permutation.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct State {
    pub xr: u32,
    pub xu: u32,
    pub tr: u32,
    pub tu: u32,
    pub odd: bool,
}

impl State {
    pub const STAGED: State = State { xr: GOAL[1], xu: GOAL[0], tr: GOAL[1], tu: GOAL[0], odd: false };

    pub fn of(c: &Cube5) -> State {
        let mask = |col: &[u8; 24], axis: u8| (0..24).fold(0u32, |m, i| m | ((col[i] % 3 == axis) as u32) << i);
        State {
            xr: mask(&c.c4.ct, 1),
            xu: mask(&c.c4.ct, 0),
            tr: mask(&c.t, 1),
            tu: mask(&c.t, 0),
            odd: c.c4.wing_parity() == 1,
        }
    }

    /// The cube of a training record.
    fn of_record(r: &[u8]) -> State {
        let mask = |o: usize, axis: u8| (0..24).fold(0u32, |m, i| m | ((r[o * 24 + i] == axis) as u32) << i);
        State { xr: mask(0, 1), xu: mask(0, 0), tr: mask(1, 1), tu: mask(1, 0), odd: r[48] == 1 }
    }

    /// Positions of the centres of each axis: x orbit, t orbit.
    #[inline(always)]
    fn masks(&self) -> [[u32; 3]; 2] {
        [[self.xu, self.xr, !(self.xu | self.xr) & ALL], [self.tu, self.tr, !(self.tu | self.tr) & ALL]]
    }
}

/// One of the three ways to hold the cube: the axis that is in place first.
struct Frame {
    /// Where each position is in this frame, x and t orbit.
    px: MaskMap,
    pt: MaskMap,
    /// The axis whose centres are the U/D ones of this frame.
    ud: usize,
    /// The move of the cube as it is for each move of the frame.
    moves: Vec<u8>,
}

/// The frame reached by turning the whole cube, and the axis that is its
/// R/L one. Found from cubes read in both frames, not from geometry.
fn frame_of(turns: &[usize]) -> (usize, Frame) {
    let g = &tables5().geometry;
    let mut seed = 0x5EED + turns.len() as u64;
    let samples: Vec<(Cube5, Cube5)> = (0..48)
        .map(|_| {
            let mut f = g.solved();
            for _ in 0..60 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                g.apply(&mut f, (seed >> 33) as usize % 36);
            }
            let read = |f: &[u8]| Cube5::from_facelets(f).expect("a scrambled cube");
            (read(&f), read(&g.reframe(&f, turns)))
        })
        .collect();
    let mut hits = Vec::new();
    for sigma in [[0u8, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        // Position `i` is position `j` of the frame if the axes agree on every cube.
        let find = |get: &dyn Fn(&Cube5) -> [u8; 24]| -> Option<Vec<u32>> {
            let (mut p, mut used) = (vec![0u32; 24], [false; 24]);
            for i in 0..24 {
                let mut fits = (0..24).filter(|&j| samples.iter().all(|(a, b)| get(b)[j] % 3 == sigma[(get(a)[i] % 3) as usize]));
                let j = fits.next()?;
                if fits.next().is_some() || used[j] {
                    return None;
                }
                used[j] = true;
                p[i] = j as u32;
            }
            Some(p)
        };
        if let (Some(px), Some(pt)) = (find(&|c| c.c4.ct), find(&|c| c.t)) {
            hits.push((sigma, px, pt));
        }
    }
    assert_eq!(hits.len(), 1, "one way to read the centres in a turned frame");
    let (sigma, px, pt) = hits.pop().unwrap();
    let axis = sigma.iter().position(|&v| v == 1).unwrap();
    let ud = sigma.iter().position(|&v| v == 0).unwrap();
    let moves = g.frame_moves(turns).into_iter().map(|m| m as u8).collect();
    (axis, Frame { px: MaskMap::new(&px), pt: MaskMap::new(&pt), ud, moves })
}

/// A symmetry of the cube as it moves the centres: where each position of
/// the x and of the t orbit goes, and which axis each axis becomes.
pub struct Sym {
    pub px: Vec<u32>,
    pub pt: Vec<u32>,
    pub axes: [usize; 3],
    mx: MaskMap,
    mt: MaskMap,
}

/// Every symmetry that keeps staged centres staged: the turns and mirror
/// images of the whole cube, 48 in all. A cube and its image are equally far
/// from staged.
fn symmetries() -> Vec<Sym> {
    use crate::sym::{axis_move_maps, closure, conjugators, destinations, renamed};
    let g = &tables5().geometry;
    let all: Vec<u8> = (0..36).collect();
    let dx = destinations(&cube_tables().ct_src);
    let dt = destinations(&tables5().t_src);
    let mut maps = axis_move_maps();
    let mut turn = [0u8; 36];
    for (m, &x) in g.frame_moves(&[0, 1]).iter().enumerate() {
        turn[m] = x as u8;
    }
    maps.push(turn);
    // The axis each axis becomes, if the map takes faces of an axis to faces of an axis.
    let axes_of = |p: &[u32]| -> Option<[usize; 3]> {
        let map = MaskMap::new(p);
        let mut axes = [0; 3];
        for k in 0..3 {
            axes[k] = GOAL.iter().position(|&goal| goal == map.apply(GOAL[k]))?;
        }
        Some(axes)
    };
    let mut generators = Vec::new();
    for mu in maps {
        let pairs = renamed(&all, &mu);
        let xa = |m: usize, p: usize| dx[m][p] as usize;
        let ta = |m: usize, p: usize| dt[m][p] as usize;
        for px in conjugators(24, &pairs, &xa, &xa, &[]) {
            for pt in conjugators(24, &pairs, &ta, &ta, &[]) {
                if let (Some(a), Some(b)) = (axes_of(&px), axes_of(&pt)) {
                    if a == b {
                        generators.push(vec![px.clone(), pt, a.iter().map(|&k| k as u32).collect()]);
                    }
                }
            }
        }
    }
    let group = closure(&generators);
    assert_eq!(group.len(), 48, "symmetries of the staged centres");
    group
        .into_iter()
        .map(|e| Sym {
            mx: MaskMap::new(&e[0]),
            mt: MaskMap::new(&e[1]),
            axes: [e[2][0] as usize, e[2][1] as usize, e[2][2] as usize],
            px: e[0].clone(),
            pt: e[1].clone(),
        })
        .collect()
}

pub struct Tools {
    pub syms: Vec<Sym>,
    bx: BitPerm,
    bt: BitPerm,
    pub c5: Centres5,
    pub stage: Stage5,
    rank8: CombRank,
    ranks: Ranks,
    /// Moves that bring the eight centres of one axis onto its faces, by
    /// orbit and axis, indexed by the rank of their mask.
    a: [[Vec<u8>; 3]; 2],
    /// By the axis that is in place.
    frames: Vec<Frame>,
}

/// A staging found by the beam: the beam cube it starts from (level, index),
/// the move after it, and the axis whose second-phase table ends it, if any.
struct Lead {
    len: usize,
    level: usize,
    parent: u32,
    mv: u8,
    frame: Option<u8>,
}

#[derive(Clone, Copy)]
struct Child {
    s: State,
    parent: u32,
    mv: u8,
}

/// What a search did: per level the cubes made, the different ones, the
/// ones scored and the ones kept.
#[derive(Default, Debug)]
pub struct Trace {
    pub levels: Vec<[usize; 4]>,
    pub scored: usize,
}

impl Tools {
    pub fn load() -> Tools {
        let bx = BitPerm::from_src(&cube_tables().ct_src);
        let bt = BitPerm::from_src(&tables5().t_src);
        let rank8 = CombRank::new(8);
        let all: Vec<u8> = (0..36).collect();
        let by_rank = |bp: &BitPerm, goal: u32| -> Vec<u8> {
            let by_mask = gen_stage(bp, goal, &all);
            rank8.unrank.iter().map(|&m| by_mask[m as usize]).collect()
        };
        let a = [[0, 1, 2].map(|k| by_rank(&bx, GOAL[k])), [0, 1, 2].map(|k| by_rank(&bt, GOAL[k]))];
        let mut frames: Vec<Option<Frame>> = vec![None, None, None];
        for turns in [vec![], vec![0, 1], vec![0, 1, 0, 1]] {
            let (axis, frame) = frame_of(&turns);
            frames[axis] = Some(frame);
        }
        let frames = frames.into_iter().map(|f| f.expect("one frame for each axis")).collect();
        let tools =
            Tools { syms: symmetries(), bx, bt, c5: Centres5::load(), stage: Stage5::load(), rank8, ranks: Ranks::new(), a, frames };
        // A cube and its images have the same distances, those of an axis at the axis it becomes.
        let mut seed = 99u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        for i in 0..200 {
            let s = tools.walk(&mut rnd, 1 + i % 16);
            let d = tools.distances(&s);
            for (g, sym) in tools.syms.iter().enumerate() {
                let e = tools.distances(&tools.image(&s, g));
                let same = (0..2).all(|o| (0..3).all(|k| e[2 + o * 3 + sym.axes[k]] == d[2 + o * 3 + k]));
                assert!(e[0] == d[0] && e[1] == d[1] && same, "a symmetry keeps the distances");
            }
        }
        tools
    }

    #[inline(always)]
    pub fn apply(&self, s: &State, m: u8) -> State {
        State {
            xr: self.bx.apply(m, s.xr),
            xu: self.bx.apply(m, s.xu),
            tr: self.bt.apply(m, s.tr),
            tu: self.bt.apply(m, s.tu),
            odd: s.odd ^ (m >= 18 && m % 3 != 1),
        }
    }

    /// The eight exact distances the network is given: each orbit staged
    /// alone, then for each axis its centres of the x orbit onto their
    /// faces, then the same for the t orbit.
    #[inline(always)]
    pub fn distances(&self, s: &State) -> [u8; 8] {
        let (hx, ht) = self.c5.bounds_of(s.xr, s.xu, s.tr, s.tu);
        let m = s.masks();
        let mut d = [hx, ht, 0, 0, 0, 0, 0, 0];
        for o in 0..2 {
            for k in 0..3 {
                d[2 + o * 3 + k] = self.a[o][k][self.rank8.rank(m[o][k])];
            }
        }
        d
    }

    /// A training record of a cube and the length of the walk that made it.
    pub fn record(&self, s: &State, k: u8) -> [u8; RECORD] {
        let mut r = [0u8; RECORD];
        let m = s.masks();
        for o in 0..2 {
            for p in 0..24 {
                r[o * 24 + p] = (0..3).find(|&k| (m[o][k] >> p) & 1 == 1).unwrap() as u8;
            }
        }
        r[48] = s.odd as u8;
        r[49..57].copy_from_slice(&self.distances(s));
        r[57] = k;
        r
    }

    /// The inputs of the network for a cube, given its distances.
    #[inline(always)]
    fn inputs(s: &State, d: &[u8; 8], row: &mut [f32]) {
        row.fill(0.0);
        let m = s.masks();
        for o in 0..2 {
            for k in 0..3 {
                let mut bits = m[o][k];
                while bits != 0 {
                    row[(o * 24 + bits.trailing_zeros() as usize) * 3 + k] = 1.0;
                    bits &= bits - 1;
                }
            }
        }
        row[144] = s.odd as u8 as f32;
        for i in 0..8 {
            row[145 + i] = d[i] as f32 / 8.0;
        }
    }

    /// The axis that is in place in both orbits, if any.
    #[inline(always)]
    fn axis_in_place(s: &State) -> Option<usize> {
        let m = s.masks();
        (0..3).find(|&k| m[0][k] == GOAL[k] && m[1][k] == GOAL[k])
    }

    /// What the second centre phase needs to know in the frame of `axis`.
    #[inline(always)]
    fn b_masks(&self, s: &State, axis: usize) -> (u32, u32) {
        let f = &self.frames[axis];
        let m = s.masks();
        let (x, t) = (f.px.apply(m[0][f.ud]), f.pt.apply(m[1][f.ud]));
        assert!(x < 1 << 16 && t < 1 << 16, "the centres of the other two axes are off the faces of the one in place");
        (x, t)
    }

    #[inline(always)]
    fn b_dist(&self, s: &State, axis: usize) -> usize {
        let (x, t) = self.b_masks(s, axis);
        self.stage.b_dist(self.ranks.rank16[x as usize] as usize, self.ranks.rank16[t as usize] as usize, s.odd) as usize
    }

    /// A cube `k` random moves from staged centres, no move undoing or
    /// continuing the one before it. The first is a quarter turn of two
    /// layers: the other 24 moves leave staged centres staged, and a walk
    /// that starts with them is shorter than it says.
    pub fn walk(&self, rnd: &mut impl FnMut(usize) -> usize, k: usize) -> State {
        let (mut s, mut last) = (State::STAGED, None::<u8>);
        for _ in 0..k {
            let m = loop {
                let m = rnd(36) as u8;
                if last.map_or(m >= 18 && m % 3 != 1, |l| !skip_move(l, m)) {
                    break m;
                }
            };
            s = self.apply(&s, m);
            last = Some(m);
        }
        s
    }

    /// The largest of the exact distances, the cheap tables first: true if it is at most `max`.
    #[inline(always)]
    fn within(&self, s: &State, max: usize) -> bool {
        let m = s.masks();
        for o in 0..2 {
            for k in 0..3 {
                if self.a[o][k][self.rank8.rank(m[o][k])] as usize > max {
                    return false;
                }
            }
        }
        let (hx, ht) = self.c5.bounds_of(s.xr, s.xu, s.tr, s.tu);
        hx.max(ht) as usize <= max
    }

    /// Stages `s` in exactly `rem` moves if that can be done; the moves are
    /// added to `path` last move first.
    fn reach(&self, s: &State, rem: usize, last: u8, nodes: &mut u64, path: &mut Vec<u8>) -> bool {
        if rem == 0 {
            return *s == State::STAGED;
        }
        *nodes += 1;
        for m in 0..36u8 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let c = self.apply(s, m);
            if self.within(&c, rem - 1) && self.reach(&c, rem - 1, m, nodes, path) {
                path.push(m);
                return true;
            }
        }
        false
    }

    /// A shortest staging of a cube, if it has at most `max` moves; and the positions searched.
    pub fn exact(&self, s: &State, max: usize) -> (Option<Vec<u8>>, u64) {
        let mut nodes = 0;
        let first = *self.distances(s).iter().max().unwrap() as usize;
        let mut path = Vec::new();
        for depth in first..=max {
            if self.reach(s, depth, 255, &mut nodes, &mut path) {
                path.reverse();
                return (Some(path), nodes);
            }
        }
        (None, nodes)
    }

    /// Every way to stage `s` in exactly `rem` moves that is not a shorter
    /// staging with moves added, each appended to `path`; at most `cap`.
    fn tails(&self, s: &State, rem: usize, last: u8, path: &mut Vec<u8>, out: &mut Vec<Vec<u8>>, cap: usize) {
        for m in 0..36u8 {
            if (last != 255 && skip_move(last, m)) || out.len() >= cap {
                continue;
            }
            let c = self.apply(s, m);
            if c == State::STAGED {
                if rem == 1 {
                    out.push(path.iter().copied().chain([m]).collect());
                }
            } else if rem > 1 && self.within(&c, rem - 1) {
                path.push(m);
                self.tails(&c, rem - 1, m, path, out, cap);
                path.pop();
            }
        }
    }

    /// The image of a cube under a symmetry.
    pub fn image(&self, s: &State, g: usize) -> State {
        let sym = &self.syms[g];
        let m = s.masks();
        let mut out = [[0u32; 3]; 2];
        for k in 0..3 {
            out[0][sym.axes[k]] = sym.mx.apply(m[0][k]);
            out[1][sym.axes[k]] = sym.mt.apply(m[1][k]);
        }
        State { xr: out[0][1], xu: out[0][0], tr: out[1][1], tu: out[1][0], odd: s.odd }
    }

    /// The estimates of the network for a list of cubes.
    pub fn estimate(&self, net: &Net, states: &[State]) -> Vec<f32> {
        states
            .par_chunks(CHUNK)
            .flat_map_iter(|part| {
                let mut x = vec![0f32; part.len() * N_IN];
                for (s, row) in part.iter().zip(x.chunks_exact_mut(N_IN)) {
                    Self::inputs(s, &self.distances(s), row);
                }
                net.forward(&x, part.len())
            })
            .collect()
    }

    /// Stagings of a cube by beam search: every one found that is at most
    /// `slack` moves longer than the shortest found, as move sequences,
    /// shortest first. With no network the cubes are ranked by the sum of
    /// their distances.
    ///
    /// The second list is what an exact search of the last `tail` moves
    /// adds: from every cube the beam kept `tail` moves before the longest
    /// length asked for, every staging of the lengths asked for. The beam
    /// holds a cube once however many ways lead to it, and finishes it one
    /// way; this finds the other ways to finish.
    pub fn beam(
        &self,
        net: Option<&Net>,
        root: State,
        width: usize,
        max_len: usize,
        slack: usize,
        tail: usize,
    ) -> (Vec<Vec<u8>>, Vec<Vec<u8>>, Trace) {
        let best = AtomicUsize::new(max_len);
        let mut trace = Trace::default();
        let mut leads: Vec<Lead> = Vec::new();
        if root == State::STAGED {
            return (vec![Vec::new()], Vec::new(), trace);
        }
        let mut kept: Vec<Vec<State>> = vec![vec![root]];
        if let Some(axis) = Self::axis_in_place(&root) {
            let len = self.b_dist(&root, axis);
            best.fetch_min(len, Relaxed);
            leads.push(Lead { len, level: 0, parent: 0, mv: 255, frame: Some(axis as u8) });
        }
        // The cubes kept at each level with the move that made them, and for
        // each the cube of the level before it came from.
        let mut beam: Vec<(State, u8)> = vec![(root, 255)];
        let mut links: Vec<Vec<(u32, u8)>> = vec![vec![(0, 255)]];
        for depth in 1..=max_len {
            if depth > best.load(Relaxed) + slack || beam.is_empty() {
                break;
            }
            let mut kids: Vec<Child> = beam
                .par_iter()
                .enumerate()
                .flat_map_iter(|(i, &(s, last))| {
                    (0..36u8)
                        .filter(move |&m| last == 255 || !skip_move(last, m))
                        .map(move |m| Child { s: self.apply(&s, m), parent: i as u32, mv: m })
                })
                .collect();
            let made = kids.len();
            kids.par_sort_unstable_by_key(|c| (c.s, c.parent, c.mv));
            kids.dedup_by_key(|c| c.s);
            // Scores: lower is nearer; infinite for a cube that is not continued.
            let parts: Vec<(Vec<f32>, Vec<Lead>)> = kids
                .par_chunks(CHUNK)
                .map(|part| {
                    let mut score = vec![f32::INFINITY; part.len()];
                    let mut found = Vec::new();
                    // The cubes the network scores, each with the largest of its exact distances.
                    let mut rows: Vec<(usize, f32)> = Vec::with_capacity(part.len());
                    let mut x: Vec<f32> = Vec::new();
                    let limit = best.load(Relaxed) + slack;
                    for (i, c) in part.iter().enumerate() {
                        // Staged: anything longer through it is this staging with moves added.
                        if c.s == State::STAGED {
                            best.fetch_min(depth, Relaxed);
                            found.push(Lead { len: depth, level: depth - 1, parent: c.parent, mv: c.mv, frame: None });
                            continue;
                        }
                        if let Some(axis) = Self::axis_in_place(&c.s) {
                            let len = depth + self.b_dist(&c.s, axis);
                            if len <= best.fetch_min(len, Relaxed).min(len) + slack {
                                found.push(Lead { len, level: depth - 1, parent: c.parent, mv: c.mv, frame: Some(axis as u8) });
                            }
                        }
                        let d = self.distances(&c.s);
                        let bound = *d.iter().max().unwrap();
                        if depth + bound as usize > limit {
                            continue;
                        }
                        match net {
                            Some(_) => {
                                x.resize((rows.len() + 1) * N_IN, 0.0);
                                Self::inputs(&c.s, &d, &mut x[rows.len() * N_IN..]);
                                rows.push((i, bound as f32));
                            }
                            None => score[i] = d.iter().map(|&v| v as f32).sum(),
                        }
                    }
                    if let Some(net) = net {
                        // No cube is nearer than its exact distances say.
                        for (&(i, bound), v) in rows.iter().zip(net.forward(&x, rows.len())) {
                            score[i] = v.max(bound);
                        }
                    }
                    (score, found)
                })
                .collect();
            let mut order: Vec<(f32, u32)> = Vec::new();
            for (k, (score, found)) in parts.into_iter().enumerate() {
                order.extend(score.iter().enumerate().filter(|(_, v)| v.is_finite()).map(|(i, &v)| (v, (k * CHUNK + i) as u32)));
                leads.extend(found);
            }
            let scored = order.len();
            if order.len() > width {
                order.select_nth_unstable_by(width, |a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                order.truncate(width);
            }
            order.sort_unstable_by_key(|o| o.1);
            beam = order.iter().map(|&(_, i)| (kids[i as usize].s, kids[i as usize].mv)).collect();
            links.push(order.iter().map(|&(_, i)| (kids[i as usize].parent, kids[i as usize].mv)).collect());
            kept.push(beam.iter().map(|b| b.0).collect());
            trace.levels.push([made, kids.len(), scored, beam.len()]);
            trace.scored += scored;
        }
        let shortest = best.load(Relaxed);
        let limit = shortest + slack;
        // The moves that lead to a cube of the beam.
        let way = |level: usize, index: usize| -> Vec<u8> {
            let (mut seq, mut at) = (Vec::with_capacity(limit), index);
            for level in (1..=level).rev() {
                let (parent, mv) = links[level][at];
                seq.push(mv);
                at = parent as usize;
            }
            seq.reverse();
            seq
        };
        let mut more: Vec<Vec<u8>> = Vec::new();
        if tail > 0 && limit > tail && limit - tail < kept.len() {
            let level = limit - tail;
            more = kept[level]
                .par_iter()
                .enumerate()
                .flat_map_iter(|(i, s)| {
                    let mut out = Vec::new();
                    let last = if level == 0 { 255 } else { links[level][i].1 };
                    for rem in shortest.saturating_sub(level).max(1)..=tail {
                        if self.within(s, rem) {
                            self.tails(s, rem, last, &mut Vec::new(), &mut out, 10_000);
                        }
                    }
                    let head = if out.is_empty() { Vec::new() } else { way(level, i) };
                    out.into_iter().map(move |t| head.iter().copied().chain(t).collect::<Vec<u8>>())
                })
                .collect();
            more.par_sort_unstable_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
            more.dedup();
        }
        let mut out: Vec<Vec<u8>> = leads
            .par_iter()
            .filter(|l| l.len <= limit)
            .map(|l| {
                let mut seq = way(l.level, l.parent as usize);
                if l.mv != 255 {
                    seq.push(l.mv);
                }
                if let Some(axis) = l.frame {
                    let s = seq.iter().fold(root, |s, &m| self.apply(&s, m));
                    let (x, t) = self.b_masks(&s, axis as usize);
                    let f = &self.frames[axis as usize];
                    seq.extend(self.stage.phase_b(x, t, s.odd).iter().map(|&m| f.moves[m as usize]));
                }
                assert_eq!(seq.len(), l.len, "a staging is as long as it was taken for");
                seq
            })
            .collect();
        out.par_sort_unstable_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        out.dedup();
        (out, more, trace)
    }
}

#[cfg(target_os = "macos")]
#[link(name = "Accelerate", kind = "framework")]
extern "C" {
    fn cblas_sgemm(
        order: i32,
        trans_a: i32,
        trans_b: i32,
        m: i32,
        n: i32,
        k: i32,
        alpha: f32,
        a: *const f32,
        lda: i32,
        b: *const f32,
        ldb: i32,
        beta: f32,
        c: *mut f32,
        ldc: i32,
    );
}

/// `c += a b` for row-major matrices: `a` is n by k, `b` is k by m. On a
/// Mac the system's library does it, about a hundred times faster than the
/// loop that every other machine gets.
fn add_product(a: &[f32], b: &[f32], c: &mut [f32], n: usize, k: usize, m: usize) {
    assert!(a.len() == n * k && b.len() == k * m && c.len() == n * m);
    #[cfg(target_os = "macos")]
    unsafe {
        // Row major, nothing transposed.
        cblas_sgemm(101, 111, 111, n as i32, m as i32, k as i32, 1.0, a.as_ptr(), k as i32, b.as_ptr(), m as i32, 1.0, c.as_mut_ptr(), m as i32);
    }
    #[cfg(not(target_os = "macos"))]
    add_product_plain(a, b, c, k, m);
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn add_product_plain(a: &[f32], b: &[f32], c: &mut [f32], k: usize, m: usize) {
    for (row, out) in a.chunks_exact(k).zip(c.chunks_exact_mut(m)) {
        for (&v, w) in row.iter().zip(b.chunks_exact(m)) {
            if v != 0.0 {
                for (o, &x) in out.iter_mut().zip(w) {
                    *o += v * x;
                }
            }
        }
    }
}

/// One layer: weights as inputs by outputs, and the bias.
struct Layer {
    w: Vec<f32>,
    b: Vec<f32>,
}

impl Layer {
    fn run(&self, x: &[f32], n: usize) -> Vec<f32> {
        let (m, k) = (self.b.len(), self.w.len() / self.b.len());
        let mut out: Vec<f32> = self.b.iter().copied().cycle().take(n * m).collect();
        add_product(x, &self.w, &mut out, n, k, m);
        out
    }
}

/// A residual network of fully connected layers, as `ml/centres5_net.py`
/// writes it (normalisations folded into the layers).
pub struct Net {
    first: Layer,
    blocks: Vec<(Layer, Layer)>,
    last: Layer,
}

impl Net {
    /// Reads a network and checks it on the cubes stored with it.
    pub fn load(path: &str) -> Result<Net, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path, e))?;
        if bytes.len() < 20 || &bytes[..8] != b"C5NET1\0\0" {
            return Err(format!("{}: not a network of this experiment", path));
        }
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let (n_in, width, blocks) = (word(8), word(12), word(16));
        if n_in != N_IN {
            return Err(format!("{}: {} inputs, expected {}", path, n_in, N_IN));
        }
        let mut at = 20;
        let mut floats = |n: usize| -> Result<Vec<f32>, String> {
            let end = at + n * 4;
            let part = bytes.get(at..end).ok_or("the network file is too short")?;
            at = end;
            Ok(part.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect())
        };
        let mut layer = |k: usize, m: usize| -> Result<Layer, String> { Ok(Layer { w: floats(k * m)?, b: floats(m)? }) };
        let first = layer(n_in, width)?;
        let mut pairs = Vec::new();
        for _ in 0..blocks {
            pairs.push((layer(width, width)?, layer(width, width)?));
        }
        let last = layer(width, 1)?;
        let net = Net { first, blocks: pairs, last };
        let tests = bytes.get(at..at + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
        at += 4;
        for i in 0..tests {
            let r = bytes.get(at..at + RECORD + 4).ok_or("the network file is too short")?;
            at += RECORD + 4;
            let want = f32::from_le_bytes(r[RECORD..].try_into().unwrap());
            let mut x = vec![0f32; N_IN];
            Tools::inputs(&State::of_record(r), r[49..57].try_into().unwrap(), &mut x);
            let got = net.forward(&x, 1)[0];
            if (got - want).abs() > 2e-3 {
                return Err(format!("{}: cube {} of the check gives {} here and {} where it was trained", path, i, got, want));
            }
        }
        Ok(net)
    }

    /// The estimates for `n` cubes given as rows of inputs.
    pub fn forward(&self, x: &[f32], n: usize) -> Vec<f32> {
        if n == 0 {
            return Vec::new();
        }
        let relu = |v: &mut [f32]| v.iter_mut().for_each(|x| *x = x.max(0.0));
        let mut h = self.first.run(x, n);
        relu(&mut h);
        for (a, b) in &self.blocks {
            let mut y = a.run(&h, n);
            relu(&mut y);
            let z = b.run(&y, n);
            for (h, z) in h.iter_mut().zip(&z) {
                *h = (*h + z).max(0.0);
            }
        }
        self.last.run(&h, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_products_agree() {
        let (n, k, m) = (7, 13, 5);
        let mut seed = 3u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 40) as f32 / (1u64 << 24) as f32) - 0.5
        };
        let a: Vec<f32> = (0..n * k).map(|_| rnd()).collect();
        let b: Vec<f32> = (0..k * m).map(|_| rnd()).collect();
        let (mut c, mut d) = (vec![0.25f32; n * m], vec![0.25f32; n * m]);
        add_product(&a, &b, &mut c, n, k, m);
        add_product_plain(&a, &b, &mut d, k, m);
        for (x, y) in c.iter().zip(&d) {
            assert!((x - y).abs() < 1e-5);
        }
    }
}
