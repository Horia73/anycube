//! Beam search, for the steps of the larger cubes whose lower bounds are far
//! below the true lengths.
//!
//! Three of those steps do two jobs that share few moves, so the larger of
//! the two bounds says little, and iterative deepening takes minutes where
//! it finishes at all. A beam search keeps a fixed number of states per
//! level, the ones a score likes best, and takes the first solutions it
//! reaches. They are not always the shortest: on the cubes compared, one
//! move longer than the exact search at 20,000 states per level, and
//! shorter again with more.

use crate::stage6::skip54;
use rayon::prelude::*;
use std::hash::{Hash, Hasher};

const NONE: u8 = 255;

/// A fixed hash of a state. States with the same score are kept in the
/// order of their hashes, so that what the beam keeps of them is a sample;
/// in the order of the states themselves it would be a cluster of similar ones.
fn mixed<N: Hash>(n: &N) -> u32 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    n.hash(&mut h);
    h.finish() as u32
}

/// A problem for `beam`: a state and the moves allowed.
pub trait Problem: Sync {
    type Node: Copy + Send + Sync;
    /// Moves of the cube (`face * 3 + power`, plus 18 per extra layer).
    fn moves(&self) -> &[u8];
    /// The state after the `k`-th of `moves`.
    fn next(&self, n: &Self::Node, k: usize) -> Self::Node;
    /// Never more than the moves still needed.
    fn bound(&self, n: &Self::Node) -> usize;
    /// Asked only where the bound is 0.
    fn done(&self, n: &Self::Node) -> bool;
}

/// Level by level, only the `width` states with the lowest `score` are
/// continued (states reached twice count once). Returns up to `cap`
/// solutions of the first length at which any is found, or nothing if there
/// is none within `max_len` moves.
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
        // (score, hash, state, parent, move); a score of 0 marks a solution.
        let mut next: Vec<(u32, u32, P::Node, u32, u8)> = level
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, (n, last))| {
                let moves = p.moves();
                let score = &score;
                (0..moves.len()).filter(move |&k| *last == NONE || !skip54(*last, moves[k])).map(move |k| {
                    let c = p.next(n, k);
                    let solved = p.bound(&c) == 0 && p.done(&c);
                    (if solved { 0 } else { 1 + score(&c) }, mixed(&c), c, i as u32, moves[k])
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
                    let mut path = vec![x.4];
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
        trail.push(next.iter().map(|x| (x.3, x.4)).collect());
        level = next.iter().map(|x| (x.2, x.4)).collect();
    }
    Vec::new()
}
