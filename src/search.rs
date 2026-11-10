//! Two searches shared by the steps of the larger cubes: iterative
//! deepening for problems with a good lower bound, and beam search for the
//! ones whose bound is far below the true length.

use crate::stage6::skip54;
use rayon::prelude::*;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed};

const NONE: u8 = 255;
/// In `beam_many`, the states of one score fill at most one part in this many of the beam.
const SHARE: usize = 6;

/// A fixed hash of a state. States with the same score are kept in the
/// order of their hashes, so that what a beam keeps of them is a sample;
/// in the order of the states themselves it would be a cluster of similar ones.
fn mixed<N: Hash>(n: &N, salt: u32) -> u32 {
    if salt == 0 {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    salt.hash(&mut h);
    n.hash(&mut h);
    h.finish() as u32
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
    P::Node: Ord + Hash,
{
    if p.bound(root) == 0 && p.done(root) {
        return vec![Vec::new()];
    }
    // Per level: each kept state's parent in the level before and the move that led to it.
    let mut trail: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut level: Vec<(P::Node, u8)> = vec![(*root, NONE)];
    for _ in 0..max_len {
        // (score, hash, state, parent, index of the move); a score of 0 marks a solution.
        let mut next: Vec<(u32, u32, P::Node, u32, u8)> = level
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, (n, last))| {
                let moves = p.moves();
                let score = &score;
                (0..moves.len()).filter(move |&k| *last == NONE || !skip54(*last, moves[k])).map(move |k| {
                    let c = p.next(n, k);
                    let solved = p.bound(&c) == 0 && p.done(&c);
                    (if solved { 0 } else { 1 + score(&c) }, mixed(&c, 1), c, i as u32, k as u8)
                })
            })
            .collect();
        next.par_sort_unstable();
        next.dedup_by(|x, y| x.2 == y.2);
        let solved = next.iter().take_while(|x| x.0 == 0).count();
        if solved > 0 {
            return next[..solved.min(cap)]
                .iter()
                .map(|x| {
                    let mut path = vec![p.moves()[x.4 as usize]];
                    let mut at = x.3 as usize;
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
        if std::env::var_os("CUBE7_BEAM_TRACE").is_some() {
            eprintln!("      level {:2}: scores {} to {}", trail.len() + 1, next.first().map_or(0, |x| x.0), next.last().map_or(0, |x| x.0));
        }
        trail.push(next.iter().map(|x| (x.3, p.moves()[x.4 as usize])).collect());
        level = next.iter().map(|x| (x.2, p.moves()[x.4 as usize])).collect();
    }
    Vec::new()
}


/// A solution of `beam_many`: the root it starts from, and its moves.
pub struct BeamFound {
    pub root: usize,
    pub moves: Vec<u8>,
}

/// Beam search from several roots at once. As `beam`, with these additions:
/// a state that looks solved counts only if `accept` agrees with the root
/// and the moves that led to it; the search goes on for `more` levels after
/// the first solutions, so that roots that start further back get their
/// turn; and of the solutions of a level the ones with the lowest `score`
/// come first (a score may look beyond the goal of this search). Returns
/// at most `cap` solutions. Which of the states with the same score are
/// kept depends on `salt`: searches that differ only in it find different
/// solutions, of lengths that differ by a few moves. With a salt of 0 the
/// states of a score are kept in their own order and may fill the beam;
/// otherwise they are a sample and fill at most a share of it.
#[allow(clippy::too_many_arguments)]
pub fn beam_many<P: Problem>(
    p: &P,
    roots: &[P::Node],
    score: impl Fn(&P::Node) -> u32 + Sync,
    accept: impl Fn(usize, &[u8]) -> bool + Sync,
    width: usize,
    cap: usize,
    more: usize,
    max_len: usize,
    salt: u32,
) -> Vec<BeamFound>
where
    P::Node: Ord + Hash,
{
    let mut out = Vec::new();
    for (i, root) in roots.iter().enumerate() {
        if p.bound(root) == 0 && p.done(root) && accept(i, &[]) {
            out.push(BeamFound { root: i, moves: Vec::new() });
        }
    }
    if !out.is_empty() {
        return out;
    }
    // Per level: each kept state's parent in the level before (for the first
    // level: its root) and the move that led to it.
    let mut trail: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut level: Vec<(P::Node, u8)> = roots.iter().map(|r| (*r, NONE)).collect();
    let mut last_level = max_len;
    for depth in 0..max_len {
        if depth > last_level || out.len() >= cap || level.is_empty() {
            break;
        }
        // (not solved, score, hash, state, parent, move).
        let mut next: Vec<(bool, u32, u32, P::Node, u32, u8)> = level
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, (n, last))| {
                let moves = p.moves();
                let score = &score;
                (0..moves.len()).filter(move |&k| *last == NONE || !skip54(*last, moves[k])).map(move |k| {
                    let c = p.next(n, k);
                    let solved = p.bound(&c) == 0 && p.done(&c);
                    (!solved, score(&c), mixed(&c, salt), c, i as u32, moves[k])
                })
            })
            .collect();
        next.par_sort_unstable();
        next.dedup_by(|x, y| x.3 == y.3);
        let solved = next.iter().take_while(|x| !x.0).count();
        for x in &next[..solved] {
            let mut path = vec![x.5];
            let mut at = x.4 as usize;
            for step in trail.iter().rev() {
                path.push(step[at].1);
                at = step[at].0 as usize;
            }
            path.reverse();
            if accept(at, &path) {
                out.push(BeamFound { root: at, moves: path });
                if out.len() >= cap {
                    break;
                }
            }
        }
        if !out.is_empty() && last_level == max_len {
            last_level = depth + more;
        }
        next.drain(..solved);
        // No score may fill more than a share of the beam: states that
        // look one move from the goal and are not would crowd out the rest.
        let (mut kept, mut run, mut score_of_run) = (0, 0, u32::MAX);
        next.retain(|x| {
            if x.1 != score_of_run {
                (run, score_of_run) = (0, x.1);
            }
            run += 1;
            let keep = (salt == 0 || run <= (width / SHARE).max(1)) && kept < width;
            kept += keep as usize;
            keep
        });
        if std::env::var_os("CUBE7_BEAM_TRACE").is_some() {
            let b = |x: Option<&(bool, u32, u32, P::Node, u32, u8)>| x.map_or((0, 0), |x| (x.1, p.bound(&x.3)));
            eprintln!("      level {:2}: {} solved, {} kept, score and bound {:?} to {:?}", depth + 1, solved, next.len(), b(next.first()), b(next.last()));
        }
        trail.push(next.iter().map(|x| (x.4, x.5)).collect());
        level = next.iter().map(|x| (x.3, x.5)).collect();
    }
    out
}
