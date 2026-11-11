//! Exact 3x3x3 search with a length limit: finds the shortest solution of at
//! most `max_len` moves, or proves there is none. IDA* whose lower bound is
//! the exact distance, along each of the three axes, to the subgroup with
//! untwisted corners, unflipped edges and the four slice edges in place.

use crate::big2::Big2;
use crate::c3::{C3, T3};
use crate::cube::{skip_move, tables as cube_tables};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering::Relaxed};
use std::time::Instant;

pub struct Opt3<'a> {
    t: &'a T3,
    big: &'a Big2,
    /// Per axis: the move seen from that axis' frame.
    axis_move: [[u8; 18]; 3],
}

pub enum Outcome {
    Solved(Vec<u8>),
    /// No solution of at most `max_len` moves exists.
    None,
    /// The node budget ran out first.
    Aborted,
}

const BATCH: u64 = 1 << 12;

struct Ctx<'a> {
    o: &'a Opt3<'a>,
    root: C3,
    path: [u8; 32],
    nodes: u64,
    budget: &'a AtomicI64,
    aborted: bool,
    /// Set by another thread that found a solution, or checked against a deadline.
    stop: Option<&'a AtomicBool>,
    deadline: Option<Instant>,
}

impl<'a> Opt3<'a> {
    pub fn new(t: &'a T3, big: &'a Big2) -> Opt3<'a> {
        let ct = cube_tables();
        let mut axis_move = [[0u8; 18]; 3];
        for (a, (r, _, _)) in t.frames.iter().enumerate() {
            for m in 0..18 {
                axis_move[a][m] = ct.relabel[ct.rot_inv[*r]][m];
                assert!(axis_move[a][m] < 18);
            }
        }
        Opt3 { t, big, axis_move }
    }

    /// Coordinates and exact distances of `cube` along the three axes.
    pub fn prepare(&self, cube: &C3) -> Root {
        let mut st = [[0u16; 3]; 3];
        let mut d = [0u8; 3];
        for (a, (_, rinv, rot)) in self.t.frames.iter().enumerate() {
            let c = rinv.mul(cube).mul(rot);
            let (tw, fl, ss) = (c.twist(), c.flip(), self.big.ss(self.t, &c));
            st[a] = [tw as u16, fl as u16, ss as u16];
            d[a] = self.big.exact(self.t, tw, fl, ss);
        }
        Root { cube: *cube, st, d }
    }

    /// One move from a node, without pruning.
    fn child(&self, st: &[[u16; 3]; 3], d: &[u8; 3], m: u8) -> ([[u16; 3]; 3], [u8; 3]) {
        let mut nst = [[0u16; 3]; 3];
        let mut nd = [0u8; 3];
        for a in 0..3 {
            let mm = self.axis_move[a][m as usize] as usize;
            let c = [
                self.t.twist_mv[st[a][0] as usize][mm],
                self.t.flip_mv[st[a][1] as usize][mm],
                self.big.ss_mv[st[a][2] as usize][mm],
            ];
            let v = self.big.get(self.big.index(c[0] as usize, c[1] as usize, c[2] as usize));
            nd[a] = d[a] + (v + 3 - (d[a] + 2) % 3) % 3 - 1;
            nst[a] = c;
        }
        (nst, nd)
    }

    /// The optimal solution, searched on all cores, length by length, until `deadline`.
    ///
    /// `known` is the length of a solution the caller already has (21 if it has none): only
    /// shorter ones are looked for. If none is found and `proven_lower_bound` comes back as
    /// `known`, the caller's solution is optimal.
    pub fn solve_optimal(&self, root: &Root, deadline: Instant, known: usize) -> Optimal {
        if root.cube == C3::identity() {
            return Optimal { solution: Some(Vec::new()), proven_lower_bound: 0 };
        }
        let budget = AtomicI64::new(i64::MAX);
        let mut lower = root.lower_bound().max(1);
        for len in lower..known {
            // Split the search two moves deep; the first piece to find a solution stops the rest.
            let mut jobs = Vec::new();
            for m1 in 0..18u8 {
                let (s1, d1) = self.child(&root.st, &root.d, m1);
                if bound(&d1) > len - 1 {
                    continue;
                }
                if len == 1 {
                    jobs.push((vec![m1], s1, d1));
                    continue;
                }
                for m2 in 0..18u8 {
                    if skip_move(m1, m2) {
                        continue;
                    }
                    let (s2, d2) = self.child(&s1, &d1, m2);
                    if bound(&d2) <= len - 2 {
                        jobs.push((vec![m1, m2], s2, d2));
                    }
                }
            }
            let stop = AtomicBool::new(false);
            let found = jobs.par_iter().find_map_any(|(prefix, st, d)| {
                let mut ctx = Ctx {
                    o: self,
                    root: root.cube,
                    path: [0; 32],
                    nodes: 0,
                    budget: &budget,
                    aborted: false,
                    stop: Some(&stop),
                    deadline: Some(deadline),
                };
                ctx.path[..prefix.len()].copy_from_slice(prefix);
                let rem = len - prefix.len();
                let hit = if rem == 0 {
                    ctx.solved(len)
                } else {
                    ctx.dfs(st, d, rem, prefix[prefix.len() - 1], prefix.len())
                };
                if hit {
                    stop.store(true, Relaxed);
                    Some(ctx.path[..len].to_vec())
                } else {
                    None
                }
            });
            if found.is_some() {
                return Optimal { solution: found, proven_lower_bound: len };
            }
            if Instant::now() > deadline {
                break;
            }
            lower = len + 1;
        }
        Optimal { solution: None, proven_lower_bound: lower }
    }

    /// Shortest solution with `min_len..=max_len` moves, assuming none is shorter than `min_len`.
    /// `budget` is a shared node allowance; the search aborts when it is used up.
    pub fn solve(&self, root: &Root, min_len: usize, max_len: usize, budget: &AtomicI64) -> Outcome {
        if root.cube == C3::identity() {
            return Outcome::Solved(Vec::new());
        }
        let mut ctx = Ctx {
            o: self,
            root: root.cube,
            path: [0; 32],
            nodes: 0,
            budget,
            aborted: false,
            stop: None,
            deadline: None,
        };
        for len in root.lower_bound().max(min_len).max(1)..=max_len.min(31) {
            if ctx.dfs(&root.st, &root.d, len, 255, 0) {
                return Outcome::Solved(ctx.path[..len].to_vec());
            }
            if ctx.aborted {
                return Outcome::Aborted;
            }
        }
        Outcome::None
    }
}

/// Result of `Opt3::solve_optimal`.
pub struct Optimal {
    /// The shortest solution, if one was found before the deadline.
    pub solution: Option<Vec<u8>>,
    /// Every shorter length than this was searched completely without a solution.
    pub proven_lower_bound: usize,
}

pub struct Root {
    cube: C3,
    st: [[u16; 3]; 3],
    d: [u8; 3],
}

impl Root {
    /// Lower bound on the solution length.
    pub fn lower_bound(&self) -> usize {
        bound(&self.d)
    }
}

/// If all three distances are equal and non-zero, one more move is needed:
/// the last move would have to leave the domino subgroup of none of the axes.
#[inline(always)]
fn bound(d: &[u8; 3]) -> usize {
    let m = d[0].max(d[1]).max(d[2]) as usize;
    if d[0] == d[1] && d[1] == d[2] && d[0] != 0 {
        m + 1
    } else {
        m
    }
}

impl Ctx<'_> {
    fn dfs(&mut self, st: &[[u16; 3]; 3], d: &[u8; 3], rem: usize, last: u8, depth: usize) -> bool {
        let (t, big) = (self.o.t, self.o.big);
        let limit = (rem - 1) as u8;
        'moves: for m in 0..18u8 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let mut nst = [[0u16; 3]; 3];
            let mut nd = [0u8; 3];
            for a in 0..3 {
                let mm = self.o.axis_move[a][m as usize] as usize;
                let c = [
                    t.twist_mv[st[a][0] as usize][mm],
                    t.flip_mv[st[a][1] as usize][mm],
                    big.ss_mv[st[a][2] as usize][mm],
                ];
                // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
                let v = big.get(big.index(c[0] as usize, c[1] as usize, c[2] as usize));
                nd[a] = d[a] + (v + 3 - (d[a] + 2) % 3) % 3 - 1;
                if nd[a] > limit {
                    continue 'moves;
                }
                nst[a] = c;
            }
            if bound(&nd) > rem - 1 {
                continue;
            }
            self.nodes += 1;
            if self.nodes % BATCH == 0
                && (self.budget.fetch_sub(BATCH as i64, Relaxed) < 0
                    || self.stop.is_some_and(|s| s.load(Relaxed))
                    || self.deadline.is_some_and(|d| Instant::now() > d))
            {
                self.aborted = true;
                return false;
            }
            self.path[depth] = m;
            if rem == 1 {
                if self.solved(depth + 1) {
                    return true;
                }
                continue;
            }
            if self.dfs(&nst, &nd, rem - 1, m, depth + 1) {
                return true;
            }
            if self.aborted {
                return false;
            }
        }
        false
    }

    fn solved(&self, len: usize) -> bool {
        let mut c = self.root;
        for &m in &self.path[..len] {
            c = c.mul(&self.o.t.moves[m as usize]);
        }
        c == C3::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{parse_moves, Cube};

    #[test]
    #[ignore = "needs the 1.7 GB slice table"]
    fn finds_optimal_solutions() {
        let t = T3::new();
        let big = Big2::load(&t);
        let o = Opt3::new(&t, &big);
        let budget = AtomicI64::new(i64::MAX);
        for (scr, opt) in [("U", 1), ("R U R' U'", 4), ("F R U' B2 L D F' R2 U B D2", 11)] {
            let cube = Cube::from_moves(&parse_moves(scr).unwrap());
            let c3 = C3::from_cube(&cube).unwrap();
            let root = o.prepare(&c3);
            match o.solve(&root, 0, 20, &budget) {
                Outcome::Solved(sol) => {
                    let mut c = cube;
                    c.apply_seq(&sol);
                    assert_eq!(c, Cube::solved());
                    assert_eq!(sol.len(), opt, "{}", scr);
                }
                _ => panic!("no solution for {}", scr),
            }
            assert!(matches!(o.solve(&root, 0, opt - 1, &budget), Outcome::None));
        }
    }

    /// With a solution in hand only shorter ones are looked for: a longer one is beaten, the
    /// optimal one is shown to be optimal.
    #[test]
    #[ignore = "needs the 1.7 GB slice table"]
    fn a_known_solution_is_beaten_or_proven() {
        let t = T3::new();
        let big = Big2::load(&t);
        let o = Opt3::new(&t, &big);
        let far = Instant::now() + std::time::Duration::from_secs(600);
        for (scr, opt) in [("R U R' U'", 4), ("F R U' B2 L D F' R2 U B D2", 11)] {
            let cube = Cube::from_moves(&parse_moves(scr).unwrap());
            let root = o.prepare(&C3::from_cube(&cube).unwrap());
            let found = o.solve_optimal(&root, far, opt + 2);
            assert_eq!(found.solution.map(|s| s.len()), Some(opt), "{}", scr);
            let proven = o.solve_optimal(&root, far, opt);
            assert!(proven.solution.is_none() && proven.proven_lower_bound == opt, "{}", scr);
        }
    }
}
