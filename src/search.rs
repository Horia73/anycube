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
/// in the order of the states themselves it would be a cluster of similar
/// ones. The sample depends on `salt`.
fn mixed<N: Hash>(n: &N, salt: u32) -> u32 {
    let mut h = Mixer(0x9E37_79B9_7F4A_7C15 ^ salt as u64);
    n.hash(&mut h);
    (h.finish() >> 32) as u32
}

/// A small multiplicative hasher: quality enough for a sample, and several
/// times quicker than the standard one on states of a few words.
struct Mixer(u64);

impl Hasher for Mixer {
    fn finish(&self) -> u64 {
        let x = self.0 ^ (self.0 >> 29);
        x.wrapping_mul(0xBF58_476D_1CE4_E5B9)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }

    fn write_u8(&mut self, x: u8) {
        self.write_u64(x as u64);
    }

    fn write_u16(&mut self, x: u16) {
        self.write_u64(x as u64);
    }

    fn write_u32(&mut self, x: u32) {
        self.write_u64(x as u64);
    }

    fn write_usize(&mut self, x: usize) {
        self.write_u64(x as u64);
    }

    fn write_u64(&mut self, x: u64) {
        self.0 = (self.0.rotate_left(23) ^ x).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
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
///
/// Which of the states with the same score are kept depends on `salt`.
/// Searches that differ only in it find solutions a move or two apart, so
/// a few of them are worth more than one search a few times as wide.
pub fn beam<P: Problem>(
    p: &P,
    root: &P::Node,
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
    salt: u32,
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
        // The score of every state one move on; 0 marks a solution, the
        // largest value a move that need not be tried.
        let moves = p.moves();
        let scores: Vec<u16> = level
            .par_iter()
            .flat_map_iter(|(n, last)| {
                let score = &score;
                (0..moves.len()).map(move |k| {
                    if *last != NONE && skip54(*last, moves[k]) {
                        return u16::MAX;
                    }
                    let c = p.next(n, k);
                    match p.bound(&c) == 0 && p.done(&c) {
                        true => 0,
                        false => (1 + score(&c)).min(u16::MAX as u32 - 1) as u16,
                    }
                })
            })
            .collect();
        // The score at which the beam is full, with room for states reached twice.
        let most = width + width / 2;
        let mut count = vec![0usize; 1 << 16];
        for &x in &scores {
            count[x as usize] += 1;
        }
        let (mut cut, mut total) = (0usize, count[0]);
        while total < most && cut < u16::MAX as usize - 1 {
            cut += 1;
            total += count[cut];
        }
        // Only the states up to that score are built: (score, hash, state, parent, move).
        let mut next: Vec<(u32, u32, P::Node, u32, u8)> = (0..scores.len())
            .into_par_iter()
            .filter(|&j| scores[j] as usize <= cut)
            .map(|j| {
                let (i, k) = (j / moves.len(), j % moves.len());
                let c = p.next(&level[i].0, k);
                (scores[j] as u32, mixed(&c, salt), c, i as u32, moves[k])
            })
            .collect();
        // Where many share the last score, a sample of them stays. A state
        // reached twice has the same score and hash both times, so its
        // copies end up side by side.
        if next.len() > most {
            next.select_nth_unstable(most);
            next.truncate(most);
        }
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

/// `beam`, run again with other ties if it comes back empty: on rare
/// states every state kept looks close and none can be finished.
pub fn beam_again<P: Problem>(
    p: &P,
    root: &P::Node,
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
    salt: u32,
) -> Vec<Vec<u8>>
where
    P::Node: Ord + Hash,
{
    (0..4).map(|again| beam(p, root, &score, width, cap, max_len, salt + 1000 * again)).find(|sols| !sols.is_empty()).unwrap_or_default()
}
