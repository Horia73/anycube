//! Exact 3x3x3 search with a length limit: finds the shortest solution of at
//! most `max_len` moves, or proves there is none. IDA* with the exact
//! distance to the domino subgroup along all three axes as lower bound.

use crate::big1::Big1;
use crate::c3::{C3, T3};
use crate::cube::{skip_move, tables as cube_tables};
use std::sync::atomic::{AtomicI64, Ordering::Relaxed};

pub struct Opt3<'a> {
    t: &'a T3,
    big: &'a Big1,
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
}

impl<'a> Opt3<'a> {
    pub fn new(t: &'a T3, big: &'a Big1) -> Opt3<'a> {
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
            let (tw, fl, sl) = self.t.coords(&rinv.mul(cube).mul(rot));
            st[a] = [tw as u16, fl as u16, sl as u16];
            d[a] = self.big.exact(self.t, tw, fl, sl);
        }
        Root { cube: *cube, st, d }
    }

    /// Shortest solution with `min_len..=max_len` moves, assuming none is shorter than `min_len`.
    /// `budget` is a shared node allowance; the search aborts when it is used up.
    pub fn solve(&self, root: &Root, min_len: usize, max_len: usize, budget: &AtomicI64) -> Outcome {
        if root.cube == C3::identity() {
            return Outcome::Solved(Vec::new());
        }
        let mut ctx = Ctx { o: self, root: root.cube, path: [0; 32], nodes: 0, budget, aborted: false };
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
                    t.slice_mv[st[a][2] as usize][mm],
                ];
                // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
                let v = big.get(Big1::index(c[0] as usize, c[1] as usize, c[2] as usize));
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
            if self.nodes % BATCH == 0 && self.budget.fetch_sub(BATCH as i64, Relaxed) < 0 {
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
    #[ignore = "needs the 554 MB exact table"]
    fn finds_optimal_solutions() {
        let t = T3::new();
        let big = Big1::load(&t);
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
}
