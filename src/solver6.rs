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
//! A 6x6x6 has no fixed centres, so which pair of colours goes to which
//! axis is ours to choose. The cube is staged held in each of the six ways
//! that differ in this, and the shortest stagings go on.

use crate::cube::{tables as cube_tables, Cube};
use crate::cuben::Geometry;
use crate::edges5::MaskRank;
use crate::solver::Solver;
use crate::solver5::{Params5, Solver5};
use crate::stage6::{skip54, Stage6, OBL_A, OBL_B};
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
#[derive(Clone, Copy)]
struct Node3 {
    sigma: [u8; 12],
    centres: u32,
    parity: u8,
    a: u32,
    b: u32,
}

pub struct Inner6 {
    /// Wings of a 4x4x4 as masks of positions, and the moves (face turns and
    /// wide half turns) until twelve chosen wings fill the low half.
    wings: BitPerm,
    rk: MaskRank,
    split: Vec<u8>,
    moves3: Vec<Move3>,
    rank8: [u8; 256],
    /// Per axis: a lower bound, in half moves, on making the two oblique
    /// orbits alike on that axis, by their two patterns. A move's cost is
    /// shared out so that the three axes may be added: a face turn counts
    /// for its own axis, a wide half turn half for each of the other two.
    /// `FAR` where step 3 cannot pair them at all.
    pairs: [Vec<u8>; 3],
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
        Inner6 { wings, rk, split, moves3, rank8, pairs, moves2, inner2, ready }
    }

    #[inline(always)]
    fn bound2(&self, n: &Node2) -> usize {
        let mut h = self.split[self.rk.rank(n.wings)];
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
                (self.bound2(&Node2 { wings, a, b }), wings)
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
                    let mut run =
                        Run2 { s6, inner: self, accept, cap, count: &count, stop: &stop, out: Vec::new(), path: [0; 32] };
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
    fn bound(&self, solver: &Solver, n: &Node3, slack: usize) -> usize {
        let t = &solver.tabs;
        let mut halves = 0;
        for axis in 0..3 {
            let pat = |x: u32| self.rank8[((x >> (8 * axis)) & 255) as usize] as usize;
            match self.pairs[axis][pat(n.a) * 70 + pat(n.b)] {
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

    /// What step 3 will need at least, for choosing among the results of step 2.
    pub fn reduce_bound(&self, s6: &Stage6, solver: &Solver, f: &[u8], inner: &Cube) -> usize {
        self.bound(solver, &self.node3(s6, f, inner), 99)
    }

    /// Step 3: the shortest ways found (up to `cap`) from a cube with staged
    /// centres and split inner wings, with the given slack (see `bound`).
    /// `None` if the search visited more than `budget` positions.
    #[allow(clippy::too_many_arguments)]
    pub fn reduce(
        &self,
        s6: &Stage6,
        solver: &Solver,
        f: &[u8],
        inner: &Cube,
        slack: usize,
        budget: u64,
        cap: usize,
    ) -> Option<Vec<Vec<u8>>> {
        let root = self.node3(s6, f, inner);
        let nodes = AtomicU64::new(0);
        let h = self.bound(solver, &root, slack);
        if h == 0 {
            return Some(vec![Vec::new()]);
        }
        let (count, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
        let trace = std::env::var_os("CUBE6_TRACE").is_some();
        if trace {
            let t = &solver.tabs;
            eprintln!(
                "    step 3 bounds: pairing {}, centres {}, obliques (half moves) {:?}",
                t.sigma[sigma_rank(&root.sigma)],
                t.ct3[t.ct3_index(root.centres, root.parity)],
                (0..3)
                    .map(|axis| {
                        let pat = |x: u32| self.rank8[((x >> (8 * axis)) & 255) as usize] as usize;
                        self.pairs[axis][pat(root.a) * 70 + pat(root.b)]
                    })
                    .collect::<Vec<_>>()
            );
        }
        for depth in h..=h + 14 {
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

struct Run2<'a> {
    s6: &'a Stage6,
    inner: &'a Inner6,
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
        let next = Node2 {
            wings: self.inner.wings.apply(self.inner.inner2[i], n.wings),
            a: self.s6.perms[OBL_A].apply(m, n.a),
            b: self.s6.perms[OBL_B].apply(m, n.b),
        };
        let h = self.inner.bound2(&next);
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
        // Only a 3-layer half turn can be the last move that matters.
        if rem == 1 && !mv.deep {
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
        let h = self.inner.bound(self.solver, &next, self.slack);
        if h > rem - 1 {
            return false;
        }
        self.path[depth] = mv.m;
        if rem == 1 {
            if h == 0 && next.a == next.b {
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
    /// Stagings taken on to step 2, and results of step 2 taken on to step 3.
    pub tries: usize,
    pub starts: usize,
    /// Cubes taken through the 5x5x5 solver.
    pub finals: usize,
    /// Step 3: the positions it may visit from one start before moving to the next.
    pub budget: u64,
    pub outer: Params5,
}

impl Params6 {
    pub fn normal() -> Params6 {
        Params6 { width: 4, tries: 2, starts: 6, finals: 2, budget: 100_000_000, outer: Params5::normal() }
    }
}

pub struct Solution6 {
    pub moves: Vec<u8>,
    /// Moves per step: staging, splitting, inner reduction, the 5x5x5 (with its 3x3x3).
    pub lens: [usize; 4],
}

pub struct Solver6 {
    pub s6: Stage6,
    pub inner: Inner6,
    pub solver4: Solver,
    pub outer: Solver5,
    g5: Geometry,
    /// The six ways to hold the cube that put different colours on the axes:
    /// the turns, and for each move then made the move of the cube as given.
    holds: Vec<(Vec<usize>, Vec<u8>)>,
}

impl Solver6 {
    pub fn load() -> Solver6 {
        let s6 = Stage6::load();
        let inner = Inner6::new(&s6);
        let holds = [vec![], vec![0], vec![1], vec![2], vec![0, 1], vec![1, 0]]
            .into_iter()
            .map(|turns| {
                let map = s6.g.frame_moves(&turns).into_iter().map(|m| m as u8).collect();
                (turns, map)
            })
            .collect();
        Solver6 { s6, inner, solver4: Solver::new(), outer: Solver5::load(), g5: Geometry::new(5), holds }
    }

    /// Solves a cube given as 216 facelets (faces U R F D L B, row by row).
    pub fn solve(&self, facelets: &[u8], p: &Params6) -> Result<Solution6, String> {
        let g = &self.s6.g;
        let apply = |f: &[u8], moves: &[u8]| {
            let mut f = f.to_vec();
            g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            f
        };
        let inner_cube =
            |f: &[u8]| Cube::from_facelets(&g.to_string(&g.sub_facelets(f, &[0, 2, 3, 5]))).expect("four layers form a 4x4x4");
        let trace = std::env::var_os("CUBE6_TRACE").is_some();
        let clock = std::time::Instant::now();
        let lap = |what: &str| {
            if trace {
                eprintln!("  {:10} at {:6.0} ms", what, clock.elapsed().as_secs_f64() * 1e3);
            }
        };
        // Step 1, holding the cube each way: (hold, staging), shortest first.
        let starts: Vec<Vec<u8>> = self.holds.iter().map(|h| g.turned(facelets, &h.0)).collect();
        let mut staged: Vec<(usize, crate::stage6::Staged6)> = (0..self.holds.len())
            .flat_map(|h| self.s6.stage(&self.outer.stage, &starts[h], p.width).into_iter().map(move |st| (h, st)))
            .collect();
        staged.sort_by_key(|s| s.1.moves.len());
        lap("staged");
        // Step 2: many ways to split the wings of the best stagings, those that
        // leave step 3 least to do first: (lower bound on the total, hold, moves, lengths, cube).
        let mut splits: Vec<(usize, usize, Vec<u8>, [usize; 2], Vec<u8>)> = staged[..staged.len().min(p.tries)]
            .par_iter()
            .flat_map_iter(|(hold, st)| {
                let f1 = apply(&starts[*hold], &st.moves);
                // The centres and the parity of the inner cube must suit its last phase.
                let suits = |path: &[u8]| {
                    let c = inner_cube(&apply(&f1, path));
                    let t = &self.solver4.tabs;
                    t.ct3[t.ct3_index(ct3_mask(&c), ct3_parity(&c))] != crate::tables::UNREACHED
                };
                self.inner
                    .split(&self.s6, &f1, &inner_cube(&f1).ep, 16, 64, &suits)
                    .into_iter()
                    .map(|sp| {
                        let f2 = apply(&f1, &sp);
                        let bound = self.inner.reduce_bound(&self.s6, &self.solver4, &f2, &inner_cube(&f2));
                        let moves: Vec<u8> = st.moves.iter().chain(sp.iter()).copied().collect();
                        (moves.len() + bound, *hold, moves, [st.moves.len(), sp.len()], f2)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        splits.sort_by_key(|x| x.0);
        splits.truncate(p.starts);
        lap("split");
        // Step 3. How hard it is varies enormously from one start to the next,
        // so each gets a small budget and the ones that finish are kept: first
        // exactly, then with less and less slack (see `Inner6::bound`).
        let mut cands: Vec<(usize, Vec<u8>, [usize; 3], Vec<u8>)> = Vec::new();
        for slack in [99, 2, 1, 0] {
            for (_, hold, moves, lens, f2) in &splits {
                let budget = if slack == 0 { u64::MAX } else { p.budget };
                let reds = self.inner.reduce(&self.s6, &self.solver4, f2, &inner_cube(f2), slack, budget, 2);
                for red in reds.unwrap_or_default() {
                    let all: Vec<u8> = moves.iter().chain(red.iter()).copied().collect();
                    cands.push((*hold, all, [lens[0], lens[1], red.len()], apply(f2, &red)));
                }
                if cands.len() >= p.finals {
                    break;
                }
            }
            if !cands.is_empty() {
                break;
            }
        }
        lap("reduced");
        cands.sort_by_key(|c| c.1.len());
        cands.truncate(p.finals);
        let best = cands
            .par_iter()
            .filter_map(|(hold, moves, lens, f)| {
                // Step 4: the outer 5x5x5, named by the colours of its centres.
                let outer = self.g5.reframe(&g.sub_facelets(f, &[0, 1, 2, 4, 5]), &[]);
                let sol5 = self.outer.solve(&outer, &p.outer).ok()?;
                if trace {
                    eprintln!("  steps so far {:?}, outer steps {:?}", lens, sol5.lens);
                }
                // Back to the moves of the cube as it was given.
                let map = &self.holds[*hold].1;
                let all: Vec<u8> = moves.iter().chain(sol5.moves.iter()).map(|&m| map[m as usize]).collect();
                Some((all, [lens[0], lens[1], lens[2], sol5.moves.len()]))
            })
            .min_by_key(|r| r.0.len())
            .ok_or("no solution found")?;
        lap("solved");
        Ok(Solution6 { moves: best.0, lens: best.1 })
    }
}
