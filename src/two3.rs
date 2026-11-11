//! 3x3x3 two-phase search on all cores, by the length of the first phase.
//!
//! The search of `c3.rs` stops after a number of second phases, on one core.
//! This one takes every way into the domino subgroup of a given length,
//! for the three axes and for the cube and its inverse, shortest first, and
//! asks of each whether the rest can be done in fewer moves than the best
//! solution so far leaves. Three things make a way cheap:
//!
//! - the first phase follows the exact distance to the subgroup, so it
//!   never walks into a dead end, and once a position is as far from the
//!   subgroup as there are moves left, its shortest ways are read from a
//!   list instead of being searched for (`first3.rs`);
//! - those lists are read for 64 positions at a time, so that the waits for
//!   memory overlap instead of following one another;
//! - the corners' permutation is carried along, and its own distance turns
//!   most ways away before the cube is put together.
//!
//! The answer does not depend on how the work falls on the cores: of the
//! solutions of the shortest length the one of the earliest piece of work
//! is returned.

use crate::c3::{perm_rank, C3, G1_MOVES, N_PERM, N_SPERM, T3};
use crate::cube::{inv_seq, skip_move, tables as cube_tables};
use crate::first3::{First3, NEAR};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;

/// How far the first phase goes: every way of fewer than `depth` moves in
/// all six variants, and those of exactly `depth` moves in the first
/// `variants` of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effort {
    pub depth: usize,
    pub variants: usize,
}

impl Effort {
    /// A few milliseconds.
    pub const FAST: Effort = Effort { depth: 12, variants: 6 };
    /// The default: a fraction of a second.
    pub const NORMAL: Effort = Effort { depth: 14, variants: 6 };
    /// A few seconds: what `--mode full` asks for before it looks for the optimal solution.
    pub const DEEP: Effort = Effort { depth: 15, variants: 6 };
}

pub struct Two3 {
    first: First3,
    /// Corner permutation under each of the 18 moves.
    cp_mv: Vec<[u16; 18]>,
    /// Moves of the second phase that the corners alone need.
    cdist: Vec<u8>,
}

const NO_MOVE: u8 = 255;

/// Moves that may follow each move in a canonical sequence; the last entry is for no move before.
fn followers() -> [u32; 19] {
    let mut out = [(1 << 18) - 1; 19];
    for last in 0..18u8 {
        out[last as usize] = (0..18u8).filter(|&m| !skip_move(last, m)).fold(0, |a, m| a | 1 << m);
    }
    out
}

#[inline(always)]
fn axis(m: u8) -> u8 {
    m / 3 % 3
}

/// A position of the first phase: twist, flip, slice, the corners'
/// permutation, its distance to the subgroup, the move that led to it and,
/// within `NEAR` of the subgroup, the moves that lead nearer and those that
/// keep the distance.
#[derive(Clone, Copy, Default)]
struct Node {
    tw: u16,
    fl: u16,
    sl: u16,
    cp: u16,
    d: u8,
    mv: u8,
    /// One move further out than `NEAR`: where the table has the position.
    sym: u8,
    key: u32,
    down: u32,
    flat: u32,
}

/// A position that is as far from the subgroup as there are moves left, and
/// near enough for its shortest ways to be listed: where the list has it,
/// and the moves that led to it.
#[derive(Clone, Copy)]
struct Arrival {
    key: u32,
    cp: u16,
    sym: u8,
    d: u8,
    depth: u8,
    path: [u8; 23],
}

/// Arrivals are looked up a lot at a time: see `Job::flush`.
const LOT: usize = 64;

#[derive(Clone, Copy)]
struct Start {
    variant: usize,
    moves: [u8; 2],
    len: usize,
    node: Node,
}

impl Two3 {
    /// Loads the table of the first phase, building it if it is missing, and the two small
    /// tables of the corners.
    pub fn load(t: &T3) -> Two3 {
        let first = First3::load(t);
        let mut cp_mv = vec![[0u16; 18]; N_PERM];
        cp_mv.par_iter_mut().enumerate().for_each(|(p, row)| {
            let mut c = C3::identity();
            crate::c3::perm_unrank(p, &mut c.cp);
            for m in 0..18 {
                row[m] = perm_rank(&c.mul(&t.moves[m]).cp) as u16;
            }
        });
        let mut cdist = vec![u8::MAX; N_PERM];
        cdist[0] = 0;
        let mut frontier = vec![0usize];
        let mut depth = 0;
        while !frontier.is_empty() {
            depth += 1;
            let mut next = Vec::new();
            for &p in &frontier {
                for k in 0..10 {
                    let q = t.cperm_mv[p][k] as usize;
                    if cdist[q] == u8::MAX {
                        cdist[q] = depth;
                        next.push(q);
                    }
                }
            }
            frontier = next;
        }
        Two3 { first, cp_mv, cdist }
    }

    /// A short solution in outer moves (0..18); see `Effort` for how hard it looks.
    pub fn solve(&self, t: &T3, cube: &C3, effort: Effort) -> Vec<u8> {
        self.solve_counted(t, cube, effort).0
    }

    /// A solution of at most `max_len` moves, if this effort finds one. Unlike `solve`, which
    /// goes on past its effort until it has a solution, this stops there; and a first phase
    /// that leaves too few moves for the second is not followed, so a low limit is quick.
    pub fn solve_within(&self, t: &T3, cube: &C3, effort: Effort, max_len: usize) -> Option<Vec<u8>> {
        self.search(t, cube, effort, Some(max_len)).0
    }

    /// The solution and the number of ways into the subgroup that were looked at.
    pub fn solve_counted(&self, t: &T3, cube: &C3, effort: Effort) -> (Vec<u8>, u64) {
        let (sol, ways) = self.search(t, cube, effort, None);
        (sol.expect("a 3x3x3 state has a solution"), ways)
    }

    fn search(&self, t: &T3, cube: &C3, effort: Effort, max_len: Option<usize>) -> (Option<Vec<u8>>, u64) {
        if *cube == C3::identity() {
            return (Some(Vec::new()), 0);
        }
        let ct = cube_tables();
        let mut variants = Vec::new();
        for (r, rinv, rot) in &t.frames {
            let conj = rinv.mul(cube).mul(rot);
            variants.push((conj, *r, false));
            variants.push((conj.inverse(), *r, true));
        }
        let follow = followers();
        // The roots, and every start two moves in: the pieces of work of the longer lengths.
        let mut roots = Vec::new();
        let mut starts = Vec::new();
        for (v, (c, _, _)) in variants.iter().enumerate() {
            let (tw, fl, sl) = t.coords(c);
            let d = self.first.exact(t, tw, fl, sl);
            let node = Node { tw: tw as u16, fl: fl as u16, sl: sl as u16, d, cp: perm_rank(&c.cp) as u16, ..Node::default() };
            roots.push(Start { variant: v, moves: [0; 2], len: 0, node });
            for m1 in 0..18u8 {
                let n1 = self.step(t, &node, m1);
                for m2 in (0..18u8).filter(|&m2| follow[m1 as usize] >> m2 & 1 == 1) {
                    starts.push(Start { variant: v, moves: [m1, m2], len: 2, node: self.step(t, &n1, m2) });
                }
            }
        }
        // With a limit, the search starts as if a solution one move longer were in hand, found by
        // the first piece of work: nothing of that length is taken.
        let best = AtomicU64::new(max_len.map_or(u64::MAX, |len| (len as u64 + 1) << 32));
        let out: Mutex<Option<(u64, usize, Vec<u8>)>> = Mutex::new(None);
        let mut probes = 0;
        for d1 in 0..=20usize {
            let found = best.load(Relaxed);
            if found != u64::MAX && (d1 as u64 >= found >> 32 || d1 > effort.depth) {
                break;
            }
            let nv = if d1 == effort.depth { effort.variants.clamp(1, variants.len()) } else { variants.len() };
            let work: &[Start] = if d1 < 4 { &roots } else { &starts };
            probes += work
                .par_iter()
                .enumerate()
                .filter(|(_, s)| s.variant < nv && s.node.d as usize <= d1 - s.len)
                .map(|(i, s)| {
                    let mut job = Job {
                        e: self,
                        t,
                        follow: &follow,
                        variant: s.variant,
                        start: variants[s.variant].0,
                        path: [0; 40],
                        sol: [0; 40],
                        stack: Vec::with_capacity(18 * 20),
                        arrivals: Vec::with_capacity(LOT),
                        best: &best,
                        out: &out,
                        rank: (d1 as u64) << 24 | i as u64,
                        probes: 0,
                    };
                    job.path[..s.len].copy_from_slice(&s.moves[..s.len]);
                    let (key, sym) = self.first.key(s.node.tw as usize, s.node.fl as usize, s.node.sl as usize);
                    let node = Node { mv: if s.len == 0 { NO_MOVE } else { s.moves[s.len - 1] }, ..s.node };
                    job.arrive(node, key, sym, d1 - s.len, s.len);
                    while let Some(n) = job.stack.pop() {
                        job.search(&n, d1 - s.len, s.len);
                    }
                    job.flush();
                    job.probes
                })
                .sum::<u64>();
        }
        let Some((_, v, mut sol)) = out.into_inner().unwrap() else { return (None, probes) };
        let (_, r, inverse) = variants[v];
        if inverse {
            sol = inv_seq(&sol);
        }
        for m in sol.iter_mut() {
            *m = ct.relabel[r][*m as usize];
        }
        (Some(sol), probes)
    }

    /// The position one move on.
    fn step(&self, t: &T3, n: &Node, m: u8) -> Node {
        let m = m as usize;
        let (tw, fl, sl) = (t.twist_mv[n.tw as usize][m], t.flip_mv[n.fl as usize][m], t.slice_mv[n.sl as usize][m]);
        let v = self.first.get(self.first.key(tw as usize, fl as usize, sl as usize).0);
        Node { tw, fl, sl, cp: self.cp_mv[n.cp as usize][m], d: nearer(n.d, v), ..Node::default() }
    }
}

/// The distance of a neighbour of a position at distance `d`, from its table entry (the distance modulo 3).
#[inline(always)]
fn nearer(d: u8, v: u8) -> u8 {
    d + (v + 3 - (d + 2) % 3) % 3 - 1
}

struct Job<'a> {
    e: &'a Two3,
    t: &'a T3,
    follow: &'a [u32; 19],
    variant: usize,
    start: C3,
    /// The moves that led to the position being searched.
    path: [u8; 40],
    /// A whole solution being tried: a first phase and the second after it.
    sol: [u8; 40],
    /// Positions of the first phase that wait to be searched.
    stack: Vec<Node>,
    /// Positions whose listed ways wait to be read.
    arrivals: Vec<Arrival>,
    /// Length, then rank, of the best solution of all jobs.
    best: &'a AtomicU64,
    out: &'a Mutex<Option<(u64, usize, Vec<u8>)>>,
    /// Length of the first phase, then the number of this piece of work.
    rank: u64,
    probes: u64,
}

impl Job<'_> {
    /// A position `depth` moves along the path, `key` and `sym` in the table, with `rem` moves
    /// left to arrive in. If that is exactly how far it is and its ways are listed, they are
    /// read later, with those of other positions; otherwise it waits on the stack to be searched.
    #[inline(always)]
    fn arrive(&mut self, mut n: Node, key: usize, sym: usize, rem: usize, depth: usize) {
        if n.d <= NEAR {
            if n.d as usize == rem {
                let mut path = [0u8; 23];
                if depth > 0 {
                    path[..depth - 1].copy_from_slice(&self.path[..depth - 1]);
                    path[depth - 1] = n.mv;
                }
                self.arrivals.push(Arrival { key: key as u32, cp: n.cp, sym: sym as u8, d: n.d, depth: depth as u8, path });
                if self.arrivals.len() == LOT {
                    self.flush();
                }
                return;
            }
            (n.down, n.flat) = self.e.first.near(key, sym, n.d);
        } else if n.d == NEAR + 1 {
            (n.key, n.sym) = (key as u32, sym as u8);
        }
        self.stack.push(n);
    }

    /// Every way from `n`, `depth` moves along the path, into the subgroup in exactly `rem` moves.
    fn search(&mut self, n: &Node, rem: usize, depth: usize) {
        if rem <= 1 {
            // In the subgroup with a move to spare: that move would be one of the second phase's.
            return;
        }
        let (e, t) = (self.e, self.t);
        let mask = self.follow[if n.mv == NO_MOVE { 18 } else { n.mv as usize }];
        let (tws, fls, sls, cps) =
            (&t.twist_mv[n.tw as usize], &t.flip_mv[n.fl as usize], &t.slice_mv[n.sl as usize], &e.cp_mv[n.cp as usize]);
        if depth > 0 {
            self.path[depth - 1] = n.mv;
        }
        // The positions one move on that can still arrive go on the stack, above the ones waiting below.
        let base = self.stack.len();
        if n.d <= NEAR {
            // The moves are listed. With one move to spare the distance may stay; with more, any move will do.
            let mut left = mask & if rem - n.d as usize == 1 { n.down | n.flat } else { !0 };
            while left != 0 {
                let m = left.trailing_zeros() as usize;
                left &= left - 1;
                let d = n.d + 1 - (n.down >> m & 1) as u8 * 2 - (n.flat >> m & 1) as u8;
                let (key, sym) = e.first.key(tws[m] as usize, fls[m] as usize, sls[m] as usize);
                let child = Node { tw: tws[m], fl: fls[m], sl: sls[m], cp: cps[m], d, mv: m as u8, ..Node::default() };
                self.arrive(child, key, sym, rem - 1, depth + 1);
            }
        } else if n.d == NEAR + 1 && rem == n.d as usize {
            // One move further out than the lists, and every move must lead nearer: those moves are listed too.
            let mut left = mask & e.first.nearer_from_next(n.key as usize, n.sym as usize);
            while left != 0 {
                let m = left.trailing_zeros() as usize;
                left &= left - 1;
                let (key, sym) = e.first.key(tws[m] as usize, fls[m] as usize, sls[m] as usize);
                let child = Node { tw: tws[m], fl: fls[m], sl: sls[m], cp: cps[m], d: NEAR, mv: m as u8, ..Node::default() };
                self.arrive(child, key, sym, rem - 1, depth + 1);
            }
        } else {
            // All the table entries first: they do not depend on one another, so the processor fetches them together.
            let mut found = [0u64; 18];
            let mut left = mask;
            while left != 0 {
                let m = left.trailing_zeros() as usize;
                left &= left - 1;
                let (key, sym) = e.first.key(tws[m] as usize, fls[m] as usize, sls[m] as usize);
                found[m] = (key as u64) << 4 | sym as u64 | (e.first.get(key) as u64) << 40;
            }
            let mut left = mask;
            while left != 0 {
                let m = left.trailing_zeros() as usize;
                left &= left - 1;
                let d = nearer(n.d, (found[m] >> 40) as u8);
                if (d as usize) < rem {
                    let child = Node { tw: tws[m], fl: fls[m], sl: sls[m], cp: cps[m], d, mv: m as u8, ..Node::default() };
                    self.arrive(child, (found[m] >> 4 & 0xFFF_FFFF) as usize, (found[m] & 15) as usize, rem - 1, depth + 1);
                }
            }
        }
        for i in base..self.stack.len() {
            let child = self.stack[i];
            self.search(&child, rem - 1, depth + 1);
        }
        self.stack.truncate(base);
    }

    /// Reads the listed ways of the positions that wait for it.
    ///
    /// Each needs two places in memory that are not in the processor's cache: its entry in the
    /// list, then its ways. One position at a time would wait for each in turn; here every
    /// entry is asked for first, then every first way, and the waits overlap.
    fn flush(&mut self) {
        let first = &self.e.first;
        let n = self.arrivals.len();
        let mut ways = [(0u32, 0u32); LOT];
        let mut touched = 0;
        for a in &self.arrivals {
            touched ^= first.peek(a.key as usize, a.d);
        }
        for (i, a) in self.arrivals.iter().enumerate() {
            ways[i] = first.ways(a.key as usize, a.d);
            touched ^= first.way(ways[i].0 as usize);
        }
        std::hint::black_box(touched);
        for i in 0..n {
            let a = self.arrivals[i];
            self.shortest_ways(&a, ways[i].0, ways[i].1);
        }
        self.arrivals.clear();
    }

    /// The listed ways of a position that is as far from the subgroup as there are moves left.
    ///
    /// They are written in the moves of the first member of the position's
    /// class, in canonical order there; renamed to the cube's own moves,
    /// two moves of opposite faces may stand the other way round. Each set
    /// of ways that differ only in such an order is still listed once.
    fn shortest_ways(&mut self, a: &Arrival, ways: u32, count: u32) {
        let e = self.e;
        let (d, depth) = (a.d as usize, a.depth as usize);
        let d1 = depth + d;
        let last = if depth == 0 { NO_MOVE } else { a.path[depth - 1] };
        let named = e.first.raw_moves(a.sym as usize);
        let wanted = self.wanted();
        for i in 0..count {
            let way = e.first.way((ways + i) as usize);
            if d > 0 && last != NO_MOVE {
                // The move before and the first one or two of the way, as one canonical sequence.
                let m1 = named[(way & 31) as usize];
                if skip_move(last, m1) || d > 1 && axis(last) == axis(m1) && axis(m1) == axis(named[(way >> 5 & 31) as usize]) {
                    continue;
                }
            }
            let (mut cp, mut left) = (a.cp as usize, way);
            for _ in 0..d {
                cp = e.cp_mv[cp][named[(left & 31) as usize] as usize] as usize;
                left >>= 5;
            }
            self.probes += 1;
            if d1 + e.cdist[cp] as usize > wanted {
                continue;
            }
            self.sol[..depth].copy_from_slice(&a.path[..depth]);
            let mut left = way;
            for j in depth..d1 {
                self.sol[j] = named[(left & 31) as usize];
                left >>= 5;
            }
            for j in depth + 1..d1 {
                if axis(self.sol[j - 1]) == axis(self.sol[j]) && self.sol[j - 1] > self.sol[j] {
                    self.sol.swap(j - 1, j);
                }
            }
            self.phase2_start(cp, d1, if d1 == 0 { NO_MOVE } else { self.sol[d1 - 1] });
        }
    }

    /// The longest solution that would still be an improvement.
    fn wanted(&self) -> usize {
        let best = self.best.load(Relaxed);
        if best == u64::MAX {
            return 40;
        }
        // Of two solutions of one length the one of the earlier piece of work is kept.
        (best >> 32) as usize - (self.rank >= best & 0xFFFF_FFFF) as usize
    }

    fn phase2_start(&mut self, cp: usize, d1: usize, last: u8) {
        let wanted = self.wanted();
        if d1 + self.e.cdist[cp] as usize > wanted {
            return;
        }
        let t = self.t;
        let c = self.sol[..d1].iter().fold(self.start, |c, &m| c.mul(&t.moves[m as usize]));
        let up = perm_rank(&c.ep[..8]);
        let mut s = [0u8; 4];
        for i in 0..4 {
            s[i] = c.ep[8 + i] - 8;
        }
        let sp = perm_rank(&s);
        let max2 = (wanted - d1).min(12);
        for d2 in t.h2(cp, up, sp) as usize..=max2 {
            if self.phase2(cp, up, sp, d2, last, d1) {
                let key = ((d1 + d2) as u64) << 32 | self.rank;
                if self.best.fetch_min(key, Relaxed) > key {
                    let mut out = self.out.lock().unwrap();
                    if out.as_ref().is_none_or(|o| key < o.0) {
                        *out = Some((key, self.variant, self.sol[..d1 + d2].to_vec()));
                    }
                }
                break;
            }
        }
    }

    fn phase2(&mut self, cp: usize, up: usize, sp: usize, rem: usize, last: u8, depth: usize) -> bool {
        if rem == 0 {
            return cp == 0 && up == 0 && sp == 0;
        }
        let t = self.t;
        for (k, &m) in G1_MOVES.iter().enumerate() {
            if last != NO_MOVE && skip_move(last, m) {
                continue;
            }
            let (a, b, c) = (t.cperm_mv[cp][k] as usize, t.uperm_mv[up][k] as usize, t.sperm_mv[sp][k] as usize);
            if t.p_cs[a * N_SPERM + c].max(t.p_us[b * N_SPERM + c]) as usize > rem - 1 {
                continue;
            }
            if t.corners_and_edges(a, b) as usize > rem - 1 {
                continue;
            }
            self.sol[depth] = m;
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
    use crate::cube::{parse_moves, Cube};

    #[test]
    fn solves_and_is_never_longer_than_the_search_on_one_core() {
        let t = T3::new();
        let two = Two3::load(&t);
        for scr in [
            "R U R' U' F2 D L' B U2 R D' F L2 B' U",
            "F R U' B2 L D F' R2 U B D2 L' F U2 R' B L U' D F2",
            "R' U2 R' L F' L R F' L2 D R2 L R L' D' L R' B' L2 U' F' D2 R2 L D' B' L' U2 D' L' B2 F R2 F' R F L' B' D B2",
            "U",
            "R2 U2",
            "R",
            "R U",
            "",
        ] {
            let cube = Cube::from_moves(&parse_moves(scr).unwrap());
            let c3 = C3::from_cube(&cube).unwrap();
            let sol = two.solve(&t, &c3, Effort { depth: 12, variants: 6 });
            let mut c = cube;
            c.apply_seq(&sol);
            assert_eq!(c, Cube::solved(), "scramble {}", scr);
            assert!(sol.len() <= t.solve(&c3, 2_000, None).len(), "scramble {}", scr);
            // The same again: the answer does not depend on the cores.
            assert_eq!(sol, two.solve(&t, &c3, Effort { depth: 12, variants: 6 }), "scramble {}", scr);
        }
    }

    /// With a limit: the same length as without when the limit allows it, nothing when it does not.
    #[test]
    fn a_limit_is_kept() {
        let t = T3::new();
        let two = Two3::load(&t);
        let effort = Effort { depth: 12, variants: 6 };
        for scr in ["F R U' B2 L D F' R2 U B D2 L' F U2 R' B L U' D F2", "R U R' U' F2 D L' B U2 R D' F L2 B' U", "R U F"] {
            let cube = Cube::from_moves(&parse_moves(scr).unwrap());
            let c3 = C3::from_cube(&cube).unwrap();
            let free = two.solve(&t, &c3, effort);
            let same = two.solve_within(&t, &c3, effort, free.len()).expect("a solution of the length found without a limit");
            assert_eq!(same.len(), free.len(), "scramble {}", scr);
            let mut c = cube;
            c.apply_seq(&same);
            assert_eq!(c, Cube::solved(), "scramble {}", scr);
            assert!(two.solve_within(&t, &c3, effort, free.len() - 1).is_none(), "scramble {}", scr);
        }
    }

    /// Random cubes, at an effort that reads the listed ways many lots at a time: every answer
    /// solves its cube, and a cube a few moves from solved is solved in no more moves than
    /// that, since every first phase that short is tried.
    #[test]
    fn random_cubes_are_solved_and_near_ones_in_no_more_moves_than_scrambled() {
        let t = T3::new();
        let two = Two3::load(&t);
        let mut seed = 2026u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        for round in 0..60 {
            let len = if round < 40 { 1 + round % 10 } else { 30 };
            let scramble: Vec<u8> = (0..len).map(|_| rnd(18) as u8).collect();
            let cube = Cube::from_moves(&scramble);
            let sol = two.solve(&t, &C3::from_cube(&cube).unwrap(), Effort { depth: 13, variants: 6 });
            let mut c = cube;
            c.apply_seq(&sol);
            assert_eq!(c, Cube::solved(), "scramble {:?}", scramble);
            assert!(sol.len() <= len.min(20), "scramble {:?} solved in {}", scramble, sol.len());
        }
    }
}
