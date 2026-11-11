//! 6x6x6, the whole solve.
//!
//! Layers 1, 3, 4 and 6 of the cube form a 4x4x4 (corners, inner wings, inner
//! x centres) whose wide turn is our 3-layer turn. Layers 1, 2, 3|4, 5 and 6
//! form a 5x5x5 once the two middle layers look alike: its centre is the
//! block of inner x centres, its t centres are the pairs of obliques, its
//! middle edges the pairs of inner wings, and its moves are our 1- and
//! 2-layer turns.
//!
//! 1. The centres are staged (`stage6`).
//! 2. The inner wings are split into the two halves of the wing positions,
//!    with face turns and wide half turns. The same search leaves the
//!    obliques so that step 3 can pair them: step 3 never turns R or L by a
//!    quarter, so on those faces the pairs that lie along a row and the
//!    pairs that lie along a column must each be fixable among themselves.
//! 3. The inner 4x4x4 is reduced with the moves of the last phase of our
//!    4x4x4 solver, and its pairing and centre tables as bounds. The same
//!    search makes every pair of obliques one colour: only 3-layer turns
//!    change which obliques are mates, and after this step there are none.
//! 4. Our 5x5x5 solver finishes.
//!
//! Step 3 does two jobs that share few moves: its lower bound is about 10
//! moves where the answer is about 17, and the exact search takes from
//! seconds to many minutes. A beam search on the sum of the two bounds
//! (`Inner6::reduce_beam`) finds 17 moves in a fraction of a second.
//!
//! The quick way (`Quads`) is what is left when the beam is off or finds
//! nothing: the obliques are first matched to the inner x centres they sit
//! next to. The moves of the inner 4x4x4 carry each quarter of a face (an
//! inner x centre and the two obliques beside it) as one piece, so once
//! every quarter is consistent, reducing the inner cube brings the obliques
//! home with the centres and no search has to watch them. That costs about
//! five moves more.

use crate::cube::{tables as cube_tables, Cube};
use crate::cuben::Geometry;
use crate::edges5::MaskRank;
use crate::solver::Solver;
use crate::solver5::{Params5, Solver5};
use crate::stage6::{skip54, Stage6, OBL_A, OBL_B, X_IN};
use crate::tables::{ct3_mask, ct3_parity, edges_pairable, sigma_move, sigma_of, sigma_rank, BitPerm, P3_MOVES};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed};

const FAR: u8 = 255;
const LOW_HALF: u32 = 0xFFF;

/// Positions of the wings a labelling sends to the low half: of every pair
/// of wings, the one whose type is the labelling's bit for that pair.
fn chosen(ep: &[u8; 24], lab: u16) -> u32 {
    (0..24).fold(0u32, |m, p| m | ((((ep[p] / 12) as u16 ^ (lab >> (ep[p] % 12))) & 1 == 0) as u32) << p)
}

/// One move of step 3: our move, and the phase-3 move of the 4x4x4 it is for the inner cube.
struct Move3 {
    m: u8,
    k: usize,
    /// A 3-layer half turn: the only kind that changes the inner pairing and the mates.
    deep: bool,
}

/// State of step 3: the inner 4x4x4 as its last phase sees it, and where
/// the obliques of each orbit show U, R or F.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Node3 {
    sigma: [u8; 12],
    centres: u32,
    parity: u8,
    a: u32,
    b: u32,
}

/// The quick way: face turns and wide half turns until, on every axis, the
/// two oblique orbits show the same pattern as the inner x centres or
/// exactly the opposite one.
///
/// A place is numbered by its quarter: `axis * 8 + side * 4 + row half * 2 +
/// column half`. Each orbit has one centre in each quarter, so three masks of
/// 24 bits (set where the centre shows U, R or F) describe the state.
pub struct Quads {
    moves: Vec<u8>,
    /// Per orbit (inner x, obliques, their mates): its facelet in each quarter.
    facelet: [[usize; 24]; 3],
    perms: [BitPerm; 3],
    rank8: [u8; 256],
    /// Per axis: a lower bound in half moves, by the three patterns, with the
    /// cost of each move shared out so that the axes may be added.
    dist: [Vec<u8>; 3],
}

impl Quads {
    pub fn new(s6: &Stage6) -> Quads {
        let n = 6;
        let moves: Vec<u8> = (0..54u8).filter(|m| m / 18 == 0 || m % 3 == 1).collect();
        let quarter = |f: usize| {
            let (face, r, c) = (f / (n * n), f / n % n, f % n);
            face % 3 * 8 + face / 3 * 4 + (r >= n / 2) as usize * 2 + (c >= n / 2) as usize
        };
        let mut facelet = [[0usize; 24]; 3];
        let mut srcs: Vec<Vec<[u8; 24]>> = Vec::new();
        for (k, &o) in [X_IN, OBL_A, OBL_B].iter().enumerate() {
            let orbit = &s6.orbits[o];
            let (mut place_of, mut position_at) = ([0usize; 24], [0usize; 24]);
            for p in 0..24 {
                place_of[p] = quarter(orbit.facelet[p]);
                facelet[k][place_of[p]] = orbit.facelet[p];
                position_at[place_of[p]] = p;
            }
            srcs.push(
                moves
                    .iter()
                    .map(|&m| {
                        let mut row = [0u8; 24];
                        for q in 0..24 {
                            row[q] = place_of[orbit.src[m as usize][position_at[q]] as usize] as u8;
                        }
                        row
                    })
                    .collect(),
            );
        }
        let perms = [BitPerm::from_src(&srcs[0]), BitPerm::from_src(&srcs[1]), BitPerm::from_src(&srcs[2])];
        let mut rank8 = [255u8; 256];
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        for (i, &x) in unrank8.iter().enumerate() {
            rank8[x as usize] = i as u8;
        }
        let axis_table = |axis: usize| -> Vec<u8> {
            let shift = 8 * axis as u32;
            let mv: Vec<Vec<Vec<u8>>> = (0..3)
                .map(|k| {
                    (0..70)
                        .map(|r| {
                            (0..moves.len())
                                .map(|i| {
                                    let y = perms[k].apply(i as u8, unrank8[r] << shift) >> shift;
                                    assert!(y < 256, "these moves keep the centres staged");
                                    rank8[y as usize]
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect();
            // In half moves: a face turn counts for its own axis, a wide half
            // turn half for each of the other two.
            let cost: Vec<u8> = moves
                .iter()
                .map(|&m| match (m >= 18, m as usize / 3 % 3 == axis) {
                    (false, true) => 2,
                    (true, false) => 1,
                    _ => 0,
                })
                .collect();
            let mut dist = vec![FAR; 70 * 70 * 70];
            for r in 0..70 {
                let opposite = rank8[(!unrank8[r] & 255) as usize] as usize;
                dist[(r * 70 + r) * 70 + r] = 0;
                dist[(r * 70 + opposite) * 70 + opposite] = 0;
            }
            loop {
                let mut changed = false;
                for s in 0..dist.len() {
                    if dist[s] == FAR {
                        continue;
                    }
                    let (x, a, b) = (s / 4900, s / 70 % 70, s % 70);
                    for i in 0..moves.len() {
                        let t = (mv[0][x][i] as usize * 70 + mv[1][a][i] as usize) * 70 + mv[2][b][i] as usize;
                        if dist[t] > dist[s] + cost[i] {
                            dist[t] = dist[s] + cost[i];
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            assert!(dist.iter().all(|&d| d != FAR), "every pattern of an axis can be matched");
            dist
        };
        let dist = [axis_table(0), axis_table(1), axis_table(2)];
        Quads { moves, facelet, perms, rank8, dist }
    }

    #[inline(always)]
    fn bound(&self, n: &[u32; 3]) -> usize {
        let mut halves = 0;
        for axis in 0..3 {
            let pat = |k: usize| self.rank8[((n[k] >> (8 * axis)) & 255) as usize] as usize;
            halves += self.dist[axis][(pat(0) * 70 + pat(1)) * 70 + pat(2)] as usize;
        }
        halves.div_ceil(2)
    }

    /// The shortest ways (up to `cap`), from a cube with staged centres.
    pub fn solve(&self, f: &[u8], cap: usize) -> Vec<Vec<u8>> {
        let root = [0, 1, 2].map(|k| (0..24).fold(0u32, |m, q| m | ((f[self.facelet[k][q]] < 3) as u32) << q));
        let h = self.bound(&root);
        if h == 0 {
            return vec![Vec::new()];
        }
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        for depth in h..=h + 12 {
            let found: Vec<Vec<u8>> = (0..self.moves.len())
                .into_par_iter()
                .flat_map_iter(|i| {
                    let mut run = QRun { q: self, cap, count: &count, stop: &stop, out: Vec::new(), path: [0; 32] };
                    run.dfs(&root, i, depth, 0);
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

struct QRun<'a> {
    q: &'a Quads,
    cap: usize,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    out: Vec<Vec<u8>>,
    path: [u8; 32],
}

impl QRun<'_> {
    fn dfs(&mut self, n: &[u32; 3], i: usize, rem: usize, depth: usize) -> bool {
        let q = self.q;
        let next = [q.perms[0].apply(i as u8, n[0]), q.perms[1].apply(i as u8, n[1]), q.perms[2].apply(i as u8, n[2])];
        let h = q.bound(&next);
        if h > rem - 1 {
            return false;
        }
        self.path[depth] = q.moves[i];
        if rem == 1 {
            self.out.push(self.path[..=depth].to_vec());
            if self.count.fetch_add(1, Relaxed) + 1 >= self.cap {
                self.stop.store(true, Relaxed);
                return true;
            }
            return false;
        }
        if h == 0 {
            return false;
        }
        if self.stop.load(Relaxed) {
            return true;
        }
        for j in 0..q.moves.len() {
            if !skip54(q.moves[i], q.moves[j]) && self.dfs(&next, j, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}

pub struct Inner6 {
    /// Wings of a 4x4x4 as masks of positions, and the moves (face turns and
    /// wide half turns) until twelve chosen wings fill the low half.
    wings: BitPerm,
    rk: MaskRank,
    split: Vec<u8>,
    moves3: Vec<Move3>,
    ids3: Vec<u8>,
    rank8: [u8; 256],
    /// Per axis: a lower bound, in half moves, on making the two oblique
    /// orbits alike on that axis, by their two patterns. A move's cost is
    /// shared out so that the three axes may be added: a face turn counts
    /// for its own axis, a wide half turn half for each of the other two.
    /// `FAR` where step 3 cannot pair them at all. The inner x centres of the
    /// axis are a third pattern: half moves until they are solved (either
    /// way round) and the obliques alike.
    joint: [Vec<u8>; 3],
    /// Step 2: its moves, the move of the inner 4x4x4 each one is, and per
    /// axis the moves until step 3 can pair the obliques of that axis.
    moves2: Vec<u8>,
    inner2: Vec<u8>,
    ready: [Vec<u8>; 3],
}

/// State of step 2: where the wings that must end in the low half are, and
/// where the obliques of each orbit show U, R or F.
#[derive(Clone, Copy)]
struct Node2 {
    wings: u32,
    a: u32,
    b: u32,
}

impl Inner6 {
    pub fn new(s6: &Stage6) -> Inner6 {
        let wings = BitPerm::from_src(&cube_tables().ep_src);
        let rk = MaskRank::new();
        let split_moves: Vec<u8> = (0..36u8).filter(|m| m / 18 == 0 || m % 3 == 1).collect();
        let mut split = vec![FAR; rk.unrank.len()];
        split[rk.rank(LOW_HALF)] = 0;
        let mut frontier = vec![LOW_HALF];
        let mut depth = 0;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &x in &frontier {
                for &m in &split_moves {
                    let y = wings.apply(m, x);
                    let r = rk.rank(y);
                    if split[r] == FAR {
                        split[r] = depth + 1;
                        next.push(y);
                    }
                }
            }
            frontier = next;
            depth += 1;
        }
        assert!(split.iter().all(|&d| d != FAR));

        let k_of = |m4: u8| P3_MOVES.iter().position(|&x| x == m4).expect("a phase-3 move");
        let mut moves3 = Vec::new();
        // Face turns the last phase of the 4x4x4 allows.
        for &m in P3_MOVES.iter().filter(|&&m| m < 18) {
            moves3.push(Move3 { m, k: k_of(m), deep: false });
        }
        // 2-layer half turns: face half turns for the inner cube.
        for face in 0..6u8 {
            moves3.push(Move3 { m: 18 + face * 3 + 1, k: k_of(face * 3 + 1), deep: false });
        }
        // 3-layer half turns: its wide half turns. The other three are these seen from behind.
        for face in 0..3u8 {
            moves3.push(Move3 { m: 36 + face * 3 + 1, k: k_of(18 + face * 3 + 1), deep: true });
        }

        let mut rank8 = [255u8; 256];
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        for (i, &x) in unrank8.iter().enumerate() {
            rank8[x as usize] = i as u8;
        }
        let axis_table = |axis: usize| -> Vec<u8> {
            let shift = 8 * axis as u32;
            let mv: Vec<Vec<Vec<u8>>> = [OBL_A, OBL_B]
                .iter()
                .map(|&o| {
                    (0..70)
                        .map(|r| {
                            moves3
                                .iter()
                                .map(|mv| {
                                    let y = s6.perms[o].apply(mv.m, unrank8[r] << shift) >> shift;
                                    assert!(y < 256, "these moves keep the centres staged");
                                    rank8[y as usize]
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect();
            // Positions are numbered U D F B R L, four to a face.
            let own = |m: u8| [0, 2, 1][m as usize / 3 % 3] == axis;
            let cost: Vec<u8> = moves3
                .iter()
                .map(|mv| match (mv.m >= 18, own(mv.m)) {
                    (false, true) => 2,
                    (true, false) => 1,
                    _ => 0,
                })
                .collect();
            let mut dist = vec![FAR; 4900];
            for r in 0..70 {
                dist[r * 70 + r] = 0;
            }
            loop {
                let mut changed = false;
                for s in 0..4900 {
                    if dist[s] == FAR {
                        continue;
                    }
                    for i in 0..moves3.len() {
                        let t = mv[0][s / 70][i] as usize * 70 + mv[1][s % 70][i] as usize;
                        if dist[t] > dist[s] + cost[i] {
                            dist[t] = dist[s] + cost[i];
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            dist
        };
        let pairs = [axis_table(0), axis_table(1), axis_table(2)];
        let centres = BitPerm::from_src(&cube_tables().ct_src);
        let joint_table = |axis: usize| -> Vec<u8> {
            let shift = 8 * axis as u32;
            let step = |k: usize, r: usize, mv: &Move3| -> usize {
                let x = unrank8[r] << shift;
                let y = match k {
                    0 => centres.apply(P3_MOVES[mv.k], x),
                    1 => s6.perms[OBL_A].apply(mv.m, x),
                    _ => s6.perms[OBL_B].apply(mv.m, x),
                } >> shift;
                rank8[y as usize] as usize
            };
            let mv: Vec<Vec<Vec<u8>>> = (0..3)
                .map(|k| (0..70).map(|r| moves3.iter().map(|m| step(k, r, m) as u8).collect()).collect())
                .collect();
            let own = |m: u8| [0, 2, 1][m as usize / 3 % 3] == axis;
            let cost: Vec<u8> = moves3
                .iter()
                .map(|mv| match (mv.m >= 18, own(mv.m)) {
                    (false, true) => 2,
                    (true, false) => 1,
                    _ => 0,
                })
                .collect();
            let mut dist = vec![FAR; 70 * 70 * 70];
            for home in [0x0Fusize, 0xF0] {
                for r in 0..70 {
                    dist[(rank8[home] as usize * 70 + r) * 70 + r] = 0;
                }
            }
            loop {
                let mut changed = false;
                for s in 0..dist.len() {
                    if dist[s] == FAR {
                        continue;
                    }
                    let (x, a, b) = (s / 4900, s / 70 % 70, s % 70);
                    for i in 0..moves3.len() {
                        let t = (mv[0][x][i] as usize * 70 + mv[1][a][i] as usize) * 70 + mv[2][b][i] as usize;
                        if dist[t] > dist[s] + cost[i] {
                            dist[t] = dist[s] + cost[i];
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            dist
        };
        let joint = [joint_table(0), joint_table(1), joint_table(2)];
        // Step 2: face turns and wide half turns.
        let moves2: Vec<u8> = (0..54u8).filter(|m| m / 18 == 0 || m % 3 == 1).collect();
        // The inner 4x4x4 feels a 2-layer turn as a face turn and a 3-layer turn as its wide turn.
        let inner2: Vec<u8> = moves2.iter().map(|&m| if m >= 18 { m - 18 } else { m }).collect();
        let ready_table = |axis: usize| -> Vec<u8> {
            let shift = 8 * axis as u32;
            let step = |o: usize, r: usize, m: u8| rank8[(s6.perms[o].apply(m, unrank8[r] << shift) >> shift) as usize] as usize;
            let mut dist: Vec<u8> = pairs[axis].iter().map(|&d| if d == FAR { FAR } else { 0 }).collect();
            let mut frontier: Vec<usize> = (0..4900).filter(|&s| dist[s] == 0).collect();
            let mut depth = 0;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &s in &frontier {
                    for &m in &moves2 {
                        let t = step(OBL_A, s / 70, m) * 70 + step(OBL_B, s % 70, m);
                        if dist[t] == FAR {
                            dist[t] = depth + 1;
                            next.push(t);
                        }
                    }
                }
                frontier = next;
                depth += 1;
            }
            assert!(dist.iter().all(|&d| d != FAR), "the obliques of an axis can always be made ready");
            dist
        };
        let ready = [ready_table(0), ready_table(1), ready_table(2)];
        let ids3 = moves3.iter().map(|mv| mv.m).collect();
        Inner6 { wings, rk, split, moves3, ids3, rank8, joint, moves2, inner2, ready }
    }

    /// `matched`: the obliques already follow the inner x centres, so only
    /// the wings matter (and 2-layer turns, which would undo that, are out).
    /// Moves needed to split the wings alone, over all labellings.
    pub fn split_bound(&self, ep: &[u8; 24]) -> usize {
        (0..4096u16)
            .filter(|lab| lab.count_ones() % 2 == 0)
            .map(|lab| self.split[self.rk.rank(chosen(ep, lab))])
            .min()
            .unwrap() as usize
    }

    #[inline(always)]
    fn bound2(&self, n: &Node2, matched: bool) -> usize {
        let mut h = self.split[self.rk.rank(n.wings)];
        if matched {
            return h as usize;
        }
        for axis in 0..3 {
            let pat = |x: u32| self.rank8[((x >> (8 * axis)) & 255) as usize] as usize;
            h = h.max(self.ready[axis][pat(n.a) * 70 + pat(n.b)]);
        }
        h as usize
    }

    /// Step 2: the shortest ways (up to `cap`), over the `labellings` most
    /// promising choices of which wing of each pair goes to the low half.
    /// `accept` has the last word on each result.
    pub fn split(
        &self,
        s6: &Stage6,
        f: &[u8],
        ep: &[u8; 24],
        matched: bool,
        labellings: usize,
        cap: usize,
        accept: &(dyn Fn(&[u8]) -> bool + Sync),
    ) -> Vec<Vec<u8>> {
        let primary = |o: usize| {
            let col = s6.orbits[o].colours(f);
            (0..24).fold(0u32, |m, i| m | ((col[i] < 3) as u32) << i)
        };
        let (a, b) = (primary(OBL_A), primary(OBL_B));
        let mut roots: Vec<(usize, u32)> = (0..4096u16)
            .filter(|lab| lab.count_ones() % 2 == 0)
            .map(|lab| {
                let wings = chosen(ep, lab);
                (self.bound2(&Node2 { wings, a, b }, matched), wings)
            })
            .collect();
        roots.sort();
        roots.truncate(labellings);
        let first = roots[0].0;
        if first == 0 && accept(&[]) {
            return vec![Vec::new()];
        }
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        for depth in first.max(1)..=first + 10 {
            if std::env::var_os("CUBE6_TRACE").is_some() {
                eprintln!("    step 2 depth {}", depth);
            }
            let jobs: Vec<(u32, usize)> = roots
                .iter()
                .filter(|r| r.0 <= depth)
                .flat_map(|r| (0..self.moves2.len()).map(move |i| (r.1, i)))
                .collect();
            let found: Vec<Vec<u8>> = jobs
                .par_iter()
                .flat_map_iter(|&(wings, i)| {
                    let mut run = Run2 {
                        s6,
                        inner: self,
                        matched,
                        accept,
                        cap,
                        count: &count,
                        stop: &stop,
                        out: Vec::new(),
                        path: [0; 32],
                    };
                    run.dfs(&Node2 { wings, a, b }, i, depth, 0);
                    run.out
                })
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// Bound for step 3. The larger of what the inner cube needs and what
    /// the obliques need is a true lower bound, but a weak one: the two
    /// share few moves (a cube with bounds 10 and 7 takes 16), and the exact
    /// search then runs for minutes. With `slack` below 99, their sum less
    /// `slack` is used as well while both are at least `slack`; near the end
    /// one move often serves both, so there the true bound is kept. This can
    /// miss the shortest way; the smaller the slack, the quicker the search
    /// and the longer its results.
    #[inline(always)]
    fn bound(&self, solver: &Solver, n: &Node3, slack: usize, obliques: bool) -> usize {
        let t = &solver.tabs;
        if !obliques {
            return (t.sigma[sigma_rank(&n.sigma)] as usize).max(t.ct3[t.ct3_index(n.centres, n.parity)] as usize);
        }
        let mut halves = 0;
        for axis in 0..3 {
            let pat = |x: u32| self.rank8[((x >> (8 * axis)) & 255) as usize] as usize;
            match self.joint[axis][(pat(n.centres) * 70 + pat(n.a)) * 70 + pat(n.b)] {
                FAR => return FAR as usize,
                d => halves += d as usize,
            }
        }
        let inner = (t.sigma[sigma_rank(&n.sigma)] as usize).max(t.ct3[t.ct3_index(n.centres, n.parity)] as usize);
        let obliques = halves.div_ceil(2);
        let both = if inner.min(obliques) >= slack { inner + obliques - slack } else { 0 };
        inner.max(obliques).max(both)
    }

    fn node3(&self, s6: &Stage6, f: &[u8], inner: &Cube) -> Node3 {
        debug_assert!(edges_pairable(inner));
        let primary = |o: usize| {
            let col = s6.orbits[o].colours(f);
            (0..24).fold(0u32, |m, i| m | ((col[i] < 3) as u32) << i)
        };
        Node3 { sigma: sigma_of(inner), centres: ct3_mask(inner), parity: ct3_parity(inner), a: primary(OBL_A), b: primary(OBL_B) }
    }

    /// What step 3 will need, for choosing among the results of step 2: the
    /// two lower bounds added. Not a lower bound itself, but the inner cube
    /// and the obliques share so few moves that it predicts the length (and
    /// with it the time) far better than the larger of the two.
    pub fn reduce_estimate(&self, s6: &Stage6, solver: &Solver, f: &[u8], inner: &Cube) -> usize {
        let n = self.node3(s6, f, inner);
        // With slack 0 the bound is the plain sum whenever both parts are non-zero.
        self.bound(solver, &n, 0, true)
    }

    /// Step 3 by beam search (see `search::beam`): the states kept are the
    /// ones for which the inner cube and the obliques together need least,
    /// their two bounds added, since they share few moves. Up to `cap`
    /// ways of the first length found; not always the shortest there is, but
    /// within a move of it at 100,000 states per level, in a fraction of a
    /// second where the exact search takes minutes.
    #[allow(clippy::too_many_arguments)]
    pub fn reduce_beam(
        &self,
        s6: &Stage6,
        solver: &Solver,
        f: &[u8],
        inner: &Cube,
        width: usize,
        cap: usize,
        salt: u32,
    ) -> Vec<Vec<u8>> {
        let root = self.node3(s6, f, inner);
        let t = &solver.tabs;
        let score = |n: &Node3| {
            let mut halves = 0u32;
            for axis in 0..3 {
                let pat = |x: u32| self.rank8[((x >> (8 * axis)) & 255) as usize] as usize;
                halves += self.joint[axis][(pat(n.centres) * 70 + pat(n.a)) * 70 + pat(n.b)] as u32;
            }
            let inner = (t.sigma[sigma_rank(&n.sigma)] as u32).max(t.ct3[t.ct3_index(n.centres, n.parity)] as u32);
            // In half moves, the inner cube weighing half as much again.
            3 * inner + halves
        };
        // No second try with other ties: a beam that fails here has run
        // onto a plateau that the others of the same start run onto as well.
        crate::search::beam(&Reduce3 { inner: self, s6, solver }, &root, score, width, cap, 40, salt)
    }

    /// Step 3: the shortest ways found (up to `cap`) from a cube with staged
    /// centres and split inner wings, with the given slack (see `bound`).
    /// `None` if the search visited more than `budget` positions.
    /// With `obliques` false they are left alone: the quick way, for a cube
    /// whose obliques already follow the inner x centres.
    #[allow(clippy::too_many_arguments)]
    pub fn reduce(
        &self,
        s6: &Stage6,
        solver: &Solver,
        f: &[u8],
        inner: &Cube,
        obliques: bool,
        slack: usize,
        budget: u64,
        cap: usize,
    ) -> Option<Vec<Vec<u8>>> {
        let root = self.node3(s6, f, inner);
        let nodes = AtomicU64::new(0);
        let h = self.bound(solver, &root, slack, obliques);
        if h == 0 && (!obliques || root.a == root.b) {
            return Some(vec![Vec::new()]);
        }
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        let trace = std::env::var_os("CUBE6_TRACE").is_some();
        if trace {
            let t = &solver.tabs;
            eprintln!(
                "    step 3 bounds: pairing {}, centres {}, centres with obliques (half moves) {:?}",
                t.sigma[sigma_rank(&root.sigma)],
                t.ct3[t.ct3_index(root.centres, root.parity)],
                (0..3)
                    .map(|axis| {
                        let pat = |x: u32| self.rank8[((x >> (8 * axis)) & 255) as usize] as usize;
                        self.joint[axis][(pat(root.centres) * 70 + pat(root.a)) * 70 + pat(root.b)]
                    })
                    .collect::<Vec<_>>()
            );
        }
        for depth in h.max(1)..=h + 14 {
            if trace {
                eprintln!("    step 3 depth {}", depth);
            }
            let found: Vec<Vec<u8>> = (0..self.moves3.len())
                .into_par_iter()
                .flat_map_iter(|i| {
                    let mut run = Run3 {
                        s6,
                        solver,
                        inner: self,
                        obliques,
                        slack,
                        budget,
                        nodes: &nodes,
                        seen: 0,
                        cap,
                        count: &count,
                        stop: &stop,
                        out: Vec::new(),
                        path: [0; 40],
                    };
                    run.dfs(&root, i, depth, 0);
                    nodes.fetch_add(run.seen, Relaxed);
                    run.out
                })
                .collect();
            if !found.is_empty() {
                return Some(found);
            }
            if nodes.load(Relaxed) > budget {
                return None;
            }
        }
        None
    }
}

/// Step 3 for a beam search.
struct Reduce3<'a> {
    inner: &'a Inner6,
    s6: &'a Stage6,
    solver: &'a Solver,
}

impl crate::search::Problem for Reduce3<'_> {
    type Node = Node3;

    fn moves(&self) -> &[u8] {
        &self.inner.ids3
    }

    fn next(&self, n: &Node3, k: usize) -> Node3 {
        let (mv, t) = (&self.inner.moves3[k], &self.solver.tabs);
        Node3 {
            sigma: sigma_move(&t.pl[mv.k], &t.phinv[mv.k], &n.sigma),
            centres: t.bp.apply(P3_MOVES[mv.k], n.centres),
            parity: n.parity ^ mv.deep as u8,
            a: self.s6.perms[OBL_A].apply(mv.m, n.a),
            b: self.s6.perms[OBL_B].apply(mv.m, n.b),
        }
    }

    fn bound(&self, n: &Node3) -> usize {
        self.inner.bound(self.solver, n, 99, true)
    }

    #[inline(always)]
    fn solved(&self, n: &Node3) -> bool {
        n.a == n.b && self.bound(n) == 0
    }

    #[inline(always)]
    fn prefetch(&self, n: &Node3) {
        // The pairing table is 240 MB; the others stay in the cache.
        crate::search::prefetch(self.solver.tabs.sigma.as_ptr().wrapping_add(sigma_rank(&n.sigma)));
    }

    fn done(&self, n: &Node3) -> bool {
        n.a == n.b
    }
}

struct Run2<'a> {
    s6: &'a Stage6,
    inner: &'a Inner6,
    matched: bool,
    accept: &'a (dyn Fn(&[u8]) -> bool + Sync),
    cap: usize,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    out: Vec<Vec<u8>>,
    path: [u8; 32],
}

impl Run2<'_> {
    fn dfs(&mut self, n: &Node2, i: usize, rem: usize, depth: usize) -> bool {
        let m = self.inner.moves2[i];
        if self.matched && (18..36).contains(&m) {
            return false;
        }
        let next = Node2 {
            wings: self.inner.wings.apply(self.inner.inner2[i], n.wings),
            a: self.s6.perms[OBL_A].apply(m, n.a),
            b: self.s6.perms[OBL_B].apply(m, n.b),
        };
        let h = self.inner.bound2(&next, self.matched);
        if h > rem - 1 {
            return false;
        }
        self.path[depth] = m;
        if rem == 1 {
            if (self.accept)(&self.path[..=depth]) {
                self.out.push(self.path[..=depth].to_vec());
                if self.count.fetch_add(1, Relaxed) + 1 >= self.cap {
                    self.stop.store(true, Relaxed);
                    return true;
                }
            }
            return false;
        }
        if self.stop.load(Relaxed) {
            return true;
        }
        for j in 0..self.inner.moves2.len() {
            if !skip54(m, self.inner.moves2[j]) && self.dfs(&next, j, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}

struct Run3<'a> {
    s6: &'a Stage6,
    solver: &'a Solver,
    inner: &'a Inner6,
    obliques: bool,
    slack: usize,
    budget: u64,
    nodes: &'a AtomicU64,
    seen: u64,
    cap: usize,
    count: &'a AtomicUsize,
    stop: &'a AtomicBool,
    out: Vec<Vec<u8>>,
    path: [u8; 40],
}

impl Run3<'_> {
    fn dfs(&mut self, n: &Node3, i: usize, rem: usize, depth: usize) -> bool {
        let mv = &self.inner.moves3[i];
        // Only a 3-layer half turn can be the last move that matters, and
        // 2-layer turns would undo a matching.
        if (rem == 1 && !mv.deep) || (!self.obliques && (18..36).contains(&mv.m)) {
            return false;
        }
        let t = &self.solver.tabs;
        let next = Node3 {
            sigma: sigma_move(&t.pl[mv.k], &t.phinv[mv.k], &n.sigma),
            centres: t.bp.apply(P3_MOVES[mv.k], n.centres),
            parity: n.parity ^ mv.deep as u8,
            a: self.s6.perms[OBL_A].apply(mv.m, n.a),
            b: self.s6.perms[OBL_B].apply(mv.m, n.b),
        };
        let h = self.inner.bound(self.solver, &next, self.slack, self.obliques);
        if h > rem - 1 {
            return false;
        }
        self.path[depth] = mv.m;
        if rem == 1 {
            if h == 0 && (!self.obliques || next.a == next.b) {
                self.out.push(self.path[..=depth].to_vec());
                if self.count.fetch_add(1, Relaxed) + 1 >= self.cap {
                    self.stop.store(true, Relaxed);
                    return true;
                }
            }
            return false;
        }
        self.seen += 1;
        if self.seen & 0xFFFF == 0 {
            if self.nodes.fetch_add(self.seen, Relaxed) + self.seen > self.budget {
                self.stop.store(true, Relaxed);
            }
            self.seen = 0;
        }
        if self.stop.load(Relaxed) {
            return true;
        }
        for j in 0..self.inner.moves3.len() {
            if !skip54(mv.m, self.inner.moves3[j].m) && self.dfs(&next, j, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}

/// How wide the search goes.
#[derive(Clone, Debug)]
pub struct Params6 {
    /// Stagings carried from step to step in step 1.
    pub width: usize,
    /// Stagings taken on to step 2.
    pub tries: usize,
    /// Cubes taken through the 5x5x5 solver.
    pub finals: usize,
    /// Positions the exact step 3 may visit for one staging; 0 skips it.
    pub budget: u64,
    /// States per level of the beam search that does step 3 otherwise; 0
    /// sends every cube the quick way.
    pub beam: usize,
    pub outer: Params5,
}

impl Params6 {
    /// A few seconds per cube. The 5x5x5 is the quick part, so it keeps
    /// more reductions than its own fast mode does.
    pub fn fast() -> Params6 {
        let outer = Params5 { e_keep: 200, pool: 60, time_ms: 1500, ..Params5::fast() };
        Params6 { width: 4, tries: 3, finals: 24, budget: 0, beam: 50_000, outer }
    }

    /// Wider, and the 5x5x5 with more reductions and more 3x3x3 searches
    /// than it takes for a 5x5x5 of its own: its choice of reductions is
    /// narrower here (they come from a few of the cubes it is given), and
    /// the 3x3x3 left over is where the candidates differ most.
    pub fn normal() -> Params6 {
        let outer =
            Params5 { c_keep: 4000, p_keep: 4000, e_keep: 800, pool: 240, probes: 10_000, time_ms: 0, ..Params5::normal() };
        Params6 { width: 8, tries: 6, finals: 64, budget: 0, beam: 100_000, outer }
    }

    /// Wider everywhere, and no clock in the 5x5x5, whose last steps get
    /// most: the 3x3x3 left over is what varies most between candidates.
    /// The exact step 3 (`budget`) stays off: it takes minutes on some
    /// cubes for less than a move.
    pub fn full() -> Params6 {
        let outer = Params5 { c_keep: 6000, p_keep: 6000, e_keep: 3000, pool: 800, probes: 10_000, time_ms: 0, ..Params5::normal() };
        Params6 { width: 16, tries: 12, finals: 128, budget: 0, beam: 300_000, outer }
    }
}

pub struct Solution6 {
    pub moves: Vec<u8>,
    /// Moves per step: staging, preparing the inner cube (splitting its
    /// wings, and matching on the quick way), inner reduction, the 5x5x5
    /// (with its 3x3x3).
    pub lens: [usize; 4],
}

pub struct Solver6 {
    pub s6: Stage6,
    pub inner: Inner6,
    pub quads: Quads,
    pub solver4: Solver,
    pub outer: Solver5,
    g5: Geometry,
}

/// A cube part of the way through: moves so far, their count per step, the cube.
pub(crate) type Partial = (Vec<u8>, [usize; 3], Vec<u8>);

impl Solver6 {
    pub fn load() -> Solver6 {
        let s6 = Stage6::load();
        let inner = Inner6::new(&s6);
        let quads = Quads::new(&s6);
        Solver6 { s6, inner, quads, solver4: Solver::new(), outer: Solver5::load(), g5: Geometry::new(5) }
    }

    fn apply(&self, f: &[u8], moves: &[u8]) -> Vec<u8> {
        let mut f = f.to_vec();
        self.s6.g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        f
    }

    fn inner_cube(&self, f: &[u8]) -> Cube {
        let g = &self.s6.g;
        Cube::from_facelets(&g.to_string(&g.sub_facelets(f, &[0, 2, 3, 5]))).expect("four layers form a 4x4x4")
    }

    /// The centres and the parity of the inner cube must suit its last phase.
    fn suits(&self, f1: &[u8], path: &[u8]) -> bool {
        let c = self.inner_cube(&self.apply(f1, path));
        let t = &self.solver4.tabs;
        t.ct3[t.ct3_index(ct3_mask(&c), ct3_parity(&c))] != crate::tables::UNREACHED
    }

    /// Step 2 from a staged cube, the results that leave step 3 least to do
    /// first: (moves, cube). `matched` is for the quick way.
    fn splits(&self, f1: &[u8], matched: bool) -> Vec<(Vec<u8>, Vec<u8>)> {
        let suits = |path: &[u8]| self.suits(f1, path);
        let mut out: Vec<(usize, Vec<u8>, Vec<u8>)> = self
            .inner
            .split(&self.s6, f1, &self.inner_cube(f1).ep, matched, 16, 64, &suits)
            .into_iter()
            .map(|sp| {
                let f2 = self.apply(f1, &sp);
                let c = self.inner_cube(&f2);
                let est = match matched {
                    true => self.inner.bound(&self.solver4, &self.inner.node3(&self.s6, &f2, &c), 99, false),
                    false => self.inner.reduce_estimate(&self.s6, &self.solver4, &f2, &c),
                };
                (sp.len() + est, sp, f2)
            })
            .collect();
        out.sort_by_key(|x| x.0);
        out.into_iter().map(|x| (x.1, x.2)).collect()
    }

    /// Steps 2 and 3 for one staging: the exact way within the budget, else
    /// the same by beam search, else (no beam, or it found nothing) the quick way.
    pub(crate) fn inner_steps(&self, staging: &[u8], f1: &[u8], budget: u64, beam: usize) -> Vec<Partial> {
        let mut out = Vec::new();
        if budget > 0 {
            if let Some((sp, f2)) = self.splits(f1, false).into_iter().next() {
                let reds = self.inner.reduce(&self.s6, &self.solver4, &f2, &self.inner_cube(&f2), true, 99, budget, 4);
                for red in reds.unwrap_or_default() {
                    let moves: Vec<u8> = staging.iter().chain(sp.iter()).chain(red.iter()).copied().collect();
                    out.push((moves, [staging.len(), sp.len(), red.len()], self.apply(&f2, &red)));
                }
            }
        }
        if out.is_empty() && beam > 0 {
            // The same step by beam search, from the two most promising splits.
            let t0 = crate::clock::cpu_ms();
            let splits = self.splits(f1, false);
            if std::env::var_os("CUBE6_TRACE").is_some() {
                eprintln!("    splits: {} in {:.0} ms of processor", splits.len(), crate::clock::cpu_ms() - t0);
            }
            for (i, (sp, f2)) in splits.into_iter().take(2).enumerate() {
                let t0 = crate::clock::cpu_ms();
                let reds = self.inner.reduce_beam(&self.s6, &self.solver4, &f2, &self.inner_cube(&f2), beam, 4, i as u32);
                if std::env::var_os("CUBE6_TRACE").is_some() {
                    eprintln!("    beam: split {}, {} results of {:?} moves in {:.0} ms of processor", sp.len(), reds.len(), reds.first().map(|r| r.len()), crate::clock::cpu_ms() - t0);
                }
                for red in reds {
                    let moves: Vec<u8> = staging.iter().chain(sp.iter()).chain(red.iter()).copied().collect();
                    out.push((moves, [staging.len(), sp.len(), red.len()], self.apply(&f2, &red)));
                }
            }
        }
        if out.is_empty() {
            // The quick way: match the obliques to the inner x centres first.
            // Of many matchings of the same length, those that leave the wings easiest to split.
            let mut matchings: Vec<(usize, Vec<u8>, Vec<u8>)> = self
                .quads
                .solve(f1, 64)
                .into_iter()
                .map(|q| {
                    let fq = self.apply(f1, &q);
                    (q.len() + self.inner.split_bound(&self.inner_cube(&fq).ep), q, fq)
                })
                .collect();
            matchings.sort_by_key(|x| x.0);
            for (_, q, fq) in matchings.into_iter().take(4) {
                // The inner reduction is quick here, so several splits are taken to the end.
                for (sp, f2) in self.splits(&fq, true).into_iter().take(4) {
                    let reds =
                        self.inner.reduce(&self.s6, &self.solver4, &f2, &self.inner_cube(&f2), false, 99, u64::MAX, 4);
                    for red in reds.unwrap_or_default() {
                        let moves: Vec<u8> =
                            staging.iter().chain(q.iter()).chain(sp.iter()).chain(red.iter()).copied().collect();
                        out.push((moves, [staging.len(), q.len() + sp.len(), red.len()], self.apply(&f2, &red)));
                    }
                }
            }
        }
        out
    }

    /// Solves a cube given as 216 facelets (faces U R F D L B, row by row).
    pub fn solve(&self, facelets: &[u8], p: &Params6) -> Result<Solution6, String> {
        let g = &self.s6.g;
        let trace = std::env::var_os("CUBE6_TRACE").is_some();
        let (clock, cpu) = (std::time::Instant::now(), crate::clock::cpu_ms());
        let lap = |what: &str| {
            if trace {
                eprintln!("  {:10} at {:6.0} ms, {:6.0} ms of processor", what, clock.elapsed().as_secs_f64() * 1e3, crate::clock::cpu_ms() - cpu);
            }
        };
        let staged = self.s6.stage(&self.outer.stage, facelets, p.width);
        lap("staged");
        let mut cands: Vec<Partial> = staged[..staged.len().min(p.tries)]
            .iter()
            .flat_map(|st| self.inner_steps(&st.moves, &self.apply(facelets, &st.moves), p.budget, p.beam))
            .collect();
        lap("reduced");
        cands.sort_by_key(|c| c.0.len());
        cands.truncate(p.finals);
        if cands.is_empty() {
            return Err("no reduction of the inner cube found".to_string());
        }
        // Step 4: the outer 5x5x5 of every candidate, named by the colours of
        // its centres; the 5x5x5 solver picks among them by total length.
        let outers: Vec<(Vec<u8>, usize)> = cands
            .iter()
            .map(|(moves, _, f)| (self.g5.reframe(&g.sub_facelets(f, &[0, 1, 2, 4, 5]), &[]), moves.len()))
            .collect();
        let (which, sol5) = self.outer.solve_any(&outers, &p.outer)?;
        let (moves, lens, _) = &cands[which];
        if trace {
            eprintln!("  steps so far {:?}, outer steps {:?}, chosen among {}", lens, sol5.lens, cands.len());
        }
        lap("solved");
        let all: Vec<u8> = moves.iter().chain(sol5.moves.iter()).copied().collect();
        Ok(Solution6 { moves: all, lens: [lens[0], lens[1], lens[2], sol5.moves.len()] })
    }
}
