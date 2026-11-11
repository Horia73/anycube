//! Centres from staged to solved, for any set of centre orbits.
//!
//! Once every centre is on a face of its own axis, face turns and half turns
//! of the deeper layers keep it there, and what is left to say about an
//! orbit is, per axis, which four of its eight places show the first colour
//! of that axis: one of 70 patterns. A face turn changes the patterns of its
//! own axis only; a deep half turn changes those of the other two axes (and
//! turns its own face). So a move's cost can be shared out between the axes
//! (a face turn counts for its own axis, a deep half turn half for each of
//! the other two) and the distances of the three axes, each read from a
//! table over a few orbits at once, add up to a lower bound.
//!
//! The larger cubes use this on small cubes made of some of their layers
//! (`nnn`): the orbits are those of a 5x5x5, 6x6x6 or 7x7x7.

use crate::orbits::CentreOrbit;
use crate::search::{beam_again, Problem};
use crate::stage6::skip54;
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

const FAR: u8 = 255;
/// Every orbit solved: the first colour of each axis on the first face of it.
const HOME: u32 = 0x0F_0F0F;
const MAX_ORBITS: usize = 6;

type Node = [u32; MAX_ORBITS];

pub struct AxisSolver {
    /// Moves of the cube this is for: face turns and half turns of deeper layers.
    moves: Vec<u8>,
    facelet: Vec<[usize; 24]>,
    perms: Vec<BitPerm>,
    rank8: [u8; 256],
    /// Orbits looked at together, and per group and axis the half moves to
    /// bring all of them home, by their patterns.
    groups: Vec<Vec<usize>>,
    dist: Vec<[Bytes; 3]>,
}

impl AxisSolver {
    /// `orbits` are those of a cube with `n_moves` moves; `groups` name the
    /// orbits (up to four each) whose patterns share a table. `name` is for
    /// the table files.
    pub fn new(name: &str, orbits: &[&CentreOrbit], n_moves: usize, groups: &[&[usize]]) -> AxisSolver {
        assert!(orbits.len() <= MAX_ORBITS);
        let moves: Vec<u8> = (0..n_moves as u8).filter(|m| m / 18 == 0 || m % 3 == 1).collect();
        let perms: Vec<BitPerm> = orbits
            .iter()
            .map(|o| BitPerm::from_src(&moves.iter().map(|&m| o.src[m as usize]).collect::<Vec<_>>()))
            .collect();
        let mut rank8 = [255u8; 256];
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        for (i, &x) in unrank8.iter().enumerate() {
            rank8[x as usize] = i as u8;
        }
        let dir = table_dir();
        let dist = groups
            .iter()
            .enumerate()
            .map(|(gi, group)| {
                [0, 1, 2].map(|axis| {
                    let size = 70usize.pow(group.len() as u32);
                    load_or_gen(&dir.join(format!("axes-{}-{}-{}.bin", name, gi, axis)), size, || {
                        gen_axis(&moves, &perms, &rank8, &unrank8, group, axis)
                    })
                })
            })
            .collect();
        AxisSolver {
            moves,
            facelet: orbits.iter().map(|o| o.facelet).collect(),
            perms,
            rank8,
            groups: groups.iter().map(|g| g.to_vec()).collect(),
            dist,
        }
    }

    fn node(&self, f: &[u8]) -> Node {
        let mut n = [HOME; MAX_ORBITS];
        for (k, cells) in self.facelet.iter().enumerate() {
            n[k] = (0..24).fold(0u32, |m, i| m | ((f[cells[i]] < 3) as u32) << i);
            assert!(
                (0..3).all(|axis| (n[k] >> (8 * axis) & 255).count_ones() == 4 && f[cells[8 * axis]] as usize % 3 == [0, 2, 1][axis]),
                "the centres must be staged"
            );
        }
        n
    }

    #[inline(always)]
    fn next(&self, n: &Node, i: usize) -> Node {
        let mut out = *n;
        for k in 0..self.perms.len() {
            out[k] = self.perms[k].apply(i as u8, n[k]);
        }
        out
    }

    /// Half moves still needed, per group.
    #[inline(always)]
    fn halves(&self, n: &Node, g: usize) -> usize {
        let mut sum = 0;
        for axis in 0..3 {
            let mut idx = 0;
            for &k in self.groups[g].iter().rev() {
                idx = idx * 70 + self.rank8[(n[k] >> (8 * axis) & 255) as usize] as usize;
            }
            sum += self.dist[g][axis][idx] as usize;
        }
        sum
    }

    #[inline(always)]
    fn bound(&self, n: &Node) -> usize {
        (0..self.groups.len()).map(|g| self.halves(n, g)).max().unwrap_or(0).div_ceil(2)
    }

    fn solved(&self, n: &Node) -> bool {
        n.iter().all(|&x| x == HOME)
    }

    /// A shortest way home from a cube with staged centres, in the moves of
    /// that cube; if that search grows past `budget` positions, a way found
    /// by beam search instead (on the sum of the groups' distances; a
    /// handful of moves above the bound with one group, about twice the
    /// bound with two). `None` if that finds nothing either.
    pub fn solve(&self, f: &[u8], budget: u64) -> Option<Vec<u8>> {
        let root = self.node(f);
        if self.solved(&root) {
            return Some(Vec::new());
        }
        let h = self.bound(&root).max(1);
        let (nodes, stop) = (AtomicU64::new(0), AtomicBool::new(false));
        for depth in h..=h + 16 {
            let found: Vec<Vec<u8>> = (0..self.moves.len())
                .into_par_iter()
                .filter_map(|i| {
                    let mut run = Run { s: self, nodes: &nodes, stop: &stop, budget, seen: 0, path: [0; 64] };
                    let hit = run.dfs(&root, i, depth, 0);
                    nodes.fetch_add(run.seen, Relaxed);
                    hit.then(|| run.path[..depth].to_vec())
                })
                .collect();
            if let Some(path) = found.into_iter().next() {
                return Some(path);
            }
            if nodes.load(Relaxed) > budget {
                break;
            }
            stop.store(false, Relaxed);
        }
        let score = |n: &Node| (0..self.groups.len()).map(|g| self.halves(n, g)).sum::<usize>() as u32;
        [(100_000, 0), (1_000_000, 1)]
            .iter()
            .map(|&(width, salt)| beam_again(&Beam { s: self }, &root, &score, width, 1, 60, salt))
            .find_map(|found| found.into_iter().next())
    }
}

struct Run<'a> {
    s: &'a AxisSolver,
    nodes: &'a AtomicU64,
    stop: &'a AtomicBool,
    budget: u64,
    seen: u64,
    path: [u8; 64],
}

impl Run<'_> {
    fn dfs(&mut self, n: &Node, i: usize, rem: usize, depth: usize) -> bool {
        let s = self.s;
        let next = s.next(n, i);
        if s.bound(&next) > rem - 1 {
            return false;
        }
        self.path[depth] = s.moves[i];
        if rem == 1 {
            if s.solved(&next) {
                self.stop.store(true, Relaxed);
                return true;
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
            return false;
        }
        for j in 0..s.moves.len() {
            if !skip54(s.moves[i], s.moves[j]) && self.dfs(&next, j, rem - 1, depth + 1) {
                return true;
            }
        }
        false
    }
}

struct Beam<'a> {
    s: &'a AxisSolver,
}

impl Problem for Beam<'_> {
    type Node = Node;

    fn moves(&self) -> &[u8] {
        &self.s.moves
    }

    fn next(&self, n: &Node, k: usize) -> Node {
        self.s.next(n, k)
    }

    fn bound(&self, n: &Node) -> usize {
        !self.s.solved(n) as usize
    }

    fn done(&self, _: &Node) -> bool {
        true
    }
}

/// Distances of one axis for one group of orbits, from all of them home.
fn gen_axis(moves: &[u8], perms: &[BitPerm], rank8: &[u8; 256], unrank8: &[u32], group: &[usize], axis: usize) -> Vec<u8> {
    let shift = 8 * axis as u32;
    // Where each move takes each pattern of each orbit of the group.
    let mv: Vec<Vec<[u8; 64]>> = group
        .iter()
        .map(|&k| {
            (0..70)
                .map(|r| {
                    let mut row = [0u8; 64];
                    for i in 0..moves.len() {
                        let y = perms[k].apply(i as u8, unrank8[r] << shift) >> shift;
                        assert!(y < 256 && rank8[y as usize] != 255, "these moves keep the centres staged");
                        row[i] = rank8[y as usize];
                    }
                    row
                })
                .collect()
        })
        .collect();
    // Positions are numbered U D F B R L, so the axes are U/D, F/B, R/L.
    let own = |m: u8| [0, 2, 1][m as usize / 3 % 3] == axis;
    let cost: Vec<u8> = moves
        .iter()
        .map(|&m| match (m >= 18, own(m)) {
            (false, true) => 2,
            (true, false) => 1,
            _ => 0,
        })
        .collect();
    // Moves that cannot change this axis at all are left out.
    let used: Vec<usize> = (0..moves.len()).filter(|&i| cost[i] > 0 || moves[i] >= 18).collect();
    let size = 70usize.pow(group.len() as u32);
    let mut dist = vec![FAR; size];
    let home = rank8[0x0F] as usize;
    let goal = (0..group.len()).fold(0, |idx, _| idx * 70 + home);
    dist[goal] = 0;
    // The costs are 0, 1 and 2: one list of states per distance.
    let mut buckets: Vec<Vec<u32>> = vec![vec![goal as u32]];
    let mut d = 0;
    let mut ranks = vec![0usize; group.len()];
    while d < buckets.len() {
        let mut at = 0;
        while at < buckets[d].len() {
            let s = buckets[d][at] as usize;
            at += 1;
            if dist[s] as usize != d {
                continue;
            }
            let mut rest = s;
            for r in ranks.iter_mut() {
                *r = rest % 70;
                rest /= 70;
            }
            for &i in &used {
                let mut t = 0;
                for j in (0..group.len()).rev() {
                    t = t * 70 + mv[j][ranks[j]][i] as usize;
                }
                let nd = d + cost[i] as usize;
                if (dist[t] as usize) > nd {
                    dist[t] = nd as u8;
                    while buckets.len() <= nd {
                        buckets.push(Vec::new());
                    }
                    buckets[nd].push(t as u32);
                }
            }
        }
        buckets[d] = Vec::new();
        d += 1;
    }
    assert!(dist.iter().all(|&x| x != FAR), "every staged pattern of an axis can be solved");
    dist
}
