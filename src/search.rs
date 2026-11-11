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
    /// Asks for the table entries that `bound` and the score of this state
    /// will read (see `fetch`). Where the tables are far larger than the
    /// cache, a state is mostly waiting for memory; asked for a few states
    /// ahead, the entries are there when they are read.
    #[inline(always)]
    fn prefetch(&self, _: &Self::Node) {}
}

/// Tells the processor that `table[index]` will be read soon.
#[inline(always)]
pub fn fetch(table: &[u8], index: usize) {
    debug_assert!(index < table.len());
    // A hint only: it reads nothing and cannot fault.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        std::arch::asm!("prfm pldl1keep, [{p}]", p = in(reg) table.as_ptr().wrapping_add(index), options(nostack, preserves_flags, readonly));
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::x86_64::_mm_prefetch(table.as_ptr().wrapping_add(index) as *const i8, std::arch::x86_64::_MM_HINT_T0);
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    let _ = (table, index);
}

/// States made and asked for (`Problem::prefetch`) before the first of them is scored.
const AHEAD: usize = 48;

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
    beam_from(p, std::slice::from_ref(root), score, width, cap, max_len, salt, &|_| true)
}

/// `beam` from several states of the same cube at once (the same position
/// seen with different aims), with `accept` deciding on each solution from
/// its moves: a state that is done but not accepted is dropped and the
/// search goes on.
#[allow(clippy::too_many_arguments)]
pub fn beam_from<P: Problem>(
    p: &P,
    roots: &[P::Node],
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
    salt: u32,
    accept: &(dyn Fn(&[u8]) -> bool + Sync),
) -> Vec<Vec<u8>>
where
    P::Node: Ord + Hash,
{
    beam_longer(p, roots, score, width, cap, max_len, salt, accept, 0)
}

/// `beam_from` that goes on for `more` levels after the first one with a
/// solution and returns the solutions of all those lengths (up to `cap` of
/// each), shortest first. For a step whose results are judged together with
/// the step after it: a solution one move longer may leave less to do.
#[allow(clippy::too_many_arguments)]
pub fn beam_longer<P: Problem>(
    p: &P,
    roots: &[P::Node],
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
    salt: u32,
    accept: &(dyn Fn(&[u8]) -> bool + Sync),
    more: usize,
) -> Vec<Vec<u8>>
where
    P::Node: Ord + Hash,
{
    let roots: Vec<(P::Node, usize)> = roots.iter().map(|root| (*root, 0)).collect();
    beam_roots(p, &roots, score, width, cap, max_len, salt, accept, more).into_iter().map(|found| found.1).collect()
}

/// One beam search for several cubes at once: each root with the number of
/// moves already spent on it beyond the least of them, so that the states of
/// a level are the same number of moves from the start whichever root they
/// come from, and the beam keeps the best of all of them. A root that is
/// `late` moves behind joins at that level. Returns the solutions as
/// `beam_longer` does, each with the number of its root.
///
/// One search this way does the work of one search per root in a fraction
/// of the time, and spends its width on the roots that turn out well.
#[allow(clippy::too_many_arguments)]
pub fn beam_roots<P: Problem>(
    p: &P,
    roots: &[(P::Node, usize)],
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
    salt: u32,
    accept: &(dyn Fn(&[u8]) -> bool + Sync),
    more: usize,
) -> Vec<(usize, Vec<u8>)>
where
    P::Node: Ord + Hash,
{
    let ready: Vec<(usize, Vec<u8>)> =
        (0..roots.len()).filter(|&i| p.bound(&roots[i].0) == 0 && p.done(&roots[i].0)).map(|i| (i, Vec::new())).collect();
    if !ready.is_empty() && accept(&[]) {
        return ready;
    }
    let last_root = roots.iter().map(|r| r.1).max().unwrap_or(0);
    let mut all: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut left = more;
    // Per level: each state's parent in the level before and the move that
    // led to it; for a root, its number and no move.
    let mut trail: Vec<Vec<(u32, u8)>> = Vec::new();
    let mut level: Vec<(P::Node, u8)> = Vec::new();
    let mut kept: Vec<(u32, u8)> = Vec::new();
    for depth in 0..max_len + last_root {
        for (i, root) in roots.iter().enumerate() {
            if root.1 == depth {
                level.push((root.0, NONE));
                kept.push((i as u32, NONE));
            }
        }
        trail.push(std::mem::take(&mut kept));
        if level.is_empty() && depth >= last_root {
            break;
        }
        // The score of every state one move on; 0 marks a solution, the
        // largest value a move that need not be tried.
        let moves = p.moves();
        let count = moves.len();
        let scores: Vec<u16> = level
            .par_chunks(256)
            .flat_map_iter(|chunk| {
                let mut out = vec![u16::MAX; chunk.len() * count];
                let mut kids: Vec<(usize, P::Node)> = Vec::with_capacity(AHEAD);
                for (i, (n, last)) in chunk.iter().enumerate() {
                    let mut k = 0;
                    while k < count {
                        kids.clear();
                        while k < count && kids.len() < AHEAD {
                            if *last == NONE || !skip54(*last, moves[k]) {
                                let c = p.next(n, k);
                                p.prefetch(&c);
                                kids.push((k, c));
                            }
                            k += 1;
                        }
                        for (k, c) in &kids {
                            out[i * count + k] = match p.bound(c) == 0 && p.done(c) {
                                true => 0,
                                false => (1 + score(c)).min(u16::MAX as u32 - 1) as u16,
                            };
                        }
                    }
                }
                out
            })
            .collect();
        // The score at which the beam is full, with room for states reached twice.
        let most = width + width / 2;
        let tally = scores
            .par_chunks(1 << 16)
            .map(|chunk| {
                let mut tally = vec![0u32; 1 << 16];
                for &x in chunk {
                    tally[x as usize] += 1;
                }
                tally
            })
            .reduce(
                || vec![0u32; 1 << 16],
                |mut a, b| {
                    for (x, y) in a.iter_mut().zip(&b) {
                        *x += y;
                    }
                    a
                },
            );
        let (mut cut, mut total) = (0usize, tally[0] as usize);
        while total < most && cut < u16::MAX as usize - 1 {
            cut += 1;
            total += tally[cut] as usize;
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
            let found: Vec<(usize, Vec<u8>)> = next[..solved]
                .iter()
                .map(|x| {
                    let mut path = vec![x.4];
                    let mut at = x.3 as usize;
                    let mut root = 0;
                    for step in trail.iter().rev() {
                        let (parent, m) = step[at];
                        if m == NONE {
                            root = parent as usize;
                            break;
                        }
                        path.push(m);
                        at = parent as usize;
                    }
                    path.reverse();
                    (root, path)
                })
                .filter(|found| accept(&found.1))
                .take(cap)
                .collect();
            let any = !found.is_empty();
            all.extend(found);
            if any && left == 0 {
                return all;
            }
            next.drain(..solved);
        }
        if !all.is_empty() {
            if left == 0 {
                return all;
            }
            left -= 1;
        }
        next.truncate(width);
        kept = next.iter().map(|x| (x.3, x.4)).collect();
        level = next.iter().map(|x| (x.2, x.4)).collect();
    }
    all
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
    beam_again_longer(p, root, score, width, cap, max_len, salt, 0)
}

/// `beam_again` with the solutions of `more` further lengths (see `beam_longer`).
#[allow(clippy::too_many_arguments)]
pub fn beam_again_longer<P: Problem>(
    p: &P,
    root: &P::Node,
    score: impl Fn(&P::Node) -> u32 + Sync,
    width: usize,
    cap: usize,
    max_len: usize,
    salt: u32,
    more: usize,
) -> Vec<Vec<u8>>
where
    P::Node: Ord + Hash,
{
    (0..4)
        .map(|again| beam_longer(p, std::slice::from_ref(root), &score, width, cap, max_len, salt + 1000 * again, &|_| true, more))
        .find(|sols| !sols.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point on a grid: U steps right, R steps up, and the goal is (3, 3).
    struct Grid;

    impl Problem for Grid {
        type Node = (i32, i32);

        fn moves(&self) -> &[u8] {
            &[0, 3]
        }

        fn next(&self, n: &(i32, i32), k: usize) -> (i32, i32) {
            match k {
                0 => (n.0 + 1, n.1),
                _ => (n.0, n.1 + 1),
            }
        }

        fn bound(&self, n: &(i32, i32)) -> usize {
            ((3 - n.0).abs() + (3 - n.1).abs()) as usize
        }

        fn done(&self, _: &(i32, i32)) -> bool {
            true
        }
    }

    fn score(n: &(i32, i32)) -> u32 {
        ((3 - n.0).abs() + (3 - n.1).abs()) as u32
    }

    #[test]
    fn a_late_root_wins_when_it_is_shorter_in_total() {
        // Six moves from the first root; four from the second, which is one move behind.
        let found = beam_roots(&Grid, &[((0, 0), 0), ((1, 1), 1)], score, 100, 10, 20, 0, &|_| true, 0);
        assert!(!found.is_empty());
        for (root, path) in &found {
            assert_eq!((*root, path.len()), (1, 4));
            let end = path.iter().fold((1, 1), |n, &m| Grid.next(&n, (m / 3) as usize));
            assert_eq!(end, (3, 3));
        }
        // Two moves behind, the two are level: the goal is one state, reached from either.
        let found = beam_roots(&Grid, &[((0, 0), 0), ((1, 1), 2)], score, 100, 10, 20, 0, &|_| true, 0);
        assert_eq!(found.len(), 1);
        assert!(matches!((found[0].0, found[0].1.len()), (0, 6) | (1, 4)));
        // Further behind still, only the first root is.
        let found = beam_roots(&Grid, &[((0, 0), 0), ((1, 1), 3)], score, 100, 10, 20, 0, &|_| true, 0);
        assert!(!found.is_empty() && found.iter().all(|x| x.0 == 0 && x.1.len() == 6));
    }

    #[test]
    fn one_more_length_on_request() {
        // The same point by two routes that differ in length cannot exist on
        // this grid, so the next length holds nothing and the first is all.
        let found = beam_longer(&Grid, &[(0, 0)], score, 100, 100, 20, 0, &|_| true, 1);
        assert!(!found.is_empty() && found.iter().all(|path| path.len() == 6));
    }
}
