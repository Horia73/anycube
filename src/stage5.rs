//! 5x5x5, first part: stage every centre on a face of its own axis and make
//! the wing permutation even. This is what phases 1 and 2 of Chen Shuang's
//! cube555 achieve.
//!
//! Phase A (all 36 moves) brings the R/L-coloured x and t centres onto the
//! R and L faces. Phase B (22 moves) separates the U/D centres from the F/B
//! ones and fixes the wing parity; its 331 million states have an exact
//! table, so every phase-A solution is scored with one lookup and the
//! shortest total wins.

use crate::c5::{tables5, Cube5};
use crate::cube::{skip_move, tables as cube_tables};
use crate::tables::{gen_stage, load_or_gen, load_or_gen_quiet, table_dir, BitPerm, Bytes, Ranks, UNREACHED};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::Relaxed};
use std::time::Instant;

const GOAL_RL: u32 = 0xFF_0000;
const GOAL_UD: u32 = 0x00_00FF;
/// U F D B face turns, R and L two-layer turns, and the other two-layer half turns.
pub const B_MOVES: [u8; 22] = [0, 1, 2, 6, 7, 8, 9, 10, 11, 15, 16, 17, 19, 25, 28, 34, 21, 22, 23, 30, 31, 32];
const N_UD: usize = 12870;
const N_B: usize = N_UD * N_UD * 2;

pub struct Stage5 {
    /// Mask permutations of the x and t centres.
    bx: BitPerm,
    bt: BitPerm,
    ranks: Ranks,
    /// Moves to bring one orbit's R/L centres onto R and L, by mask.
    ax: Bytes,
    at: Bytes,
    /// Exact length of phase B.
    b: Bytes,
    x_mv: Vec<[u16; 22]>,
    t_mv: Vec<[u16; 22]>,
    /// The three frames: whole-cube turns and the original name of each move.
    frames: Vec<(Vec<usize>, Vec<usize>)>,
    /// Phase-A lower bound near the goal; see `gen_near`.
    near: Bytes,
}

/// Phase-A lower bound near the goal. Every state within `NEAR_A` moves of
/// the goal writes its exact distance into a bucket chosen by a hash; a
/// bucket keeps the smallest value written, and an untouched bucket means
/// "further than `NEAR_A`". Four bits per bucket, about six buckets per state,
/// so most far states are recognised as far. Never more than the true distance.
const NEAR_A: usize = 6;
const NEAR_A_STATES: usize = 17_168_476;
const BUCKETS: usize = 1 << 27;

#[inline(always)]
fn bucket(x: u32, t: u32) -> usize {
    let key = (x as u64) << 24 | t as u64;
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 37) as usize & (BUCKETS - 1)
}

fn gen_near(bx: &BitPerm, bt: &BitPerm) -> Vec<u8> {
    let far = (NEAR_A + 1) as u8;
    let mut table = vec![far | far << 4; BUCKETS / 2];
    let mut put = |x: u32, t: u32, d: u8| {
        let b = bucket(x, t);
        let shift = (b & 1) * 4;
        if (table[b >> 1] >> shift) & 15 > d {
            table[b >> 1] = (table[b >> 1] & !(15 << shift)) | d << shift;
        }
    };
    let key = |x: u32, t: u32| (x as u64) << 24 | t as u64;
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    seen.insert(key(GOAL_RL, GOAL_RL));
    put(GOAL_RL, GOAL_RL, 0);
    let mut frontier = vec![(GOAL_RL, GOAL_RL)];
    for depth in 1..=NEAR_A {
        let mut next = Vec::new();
        for &(x, t) in &frontier {
            for m in 0..36u8 {
                let (a, b) = (bx.apply(m, x), bt.apply(m, t));
                if seen.insert(key(a, b)) {
                    put(a, b, depth as u8);
                    next.push((a, b));
                }
            }
        }
        frontier = next;
    }
    assert_eq!(seen.len(), NEAR_A_STATES);
    table
}

struct Ctx<'a> {
    out: &'a mut Vec<Vec<u8>>,
    limit: usize,
    nodes: u64,
    stop: &'a AtomicBool,
    deadline: Option<Instant>,
}

/// A way to stage the centres.
pub struct Staged {
    /// Moves in the original frame.
    pub moves: Vec<u8>,
    pub len_a: usize,
}

fn rl_mask(colours: &[u8; 24]) -> u32 {
    (0..24).fold(0, |m, i| m | ((colours[i] % 3 == 1) as u32) << i)
}

fn ud_mask(colours: &[u8; 24]) -> u32 {
    (0..16).fold(0, |m, i| m | ((colours[i] % 3 == 0) as u32) << i)
}

/// Wide quarter turns move one inner slice a quarter turn: an odd wing permutation.
fn flips_parity(m: u8) -> bool {
    m >= 18 && m % 3 != 1
}

impl Stage5 {
    pub fn load() -> Stage5 {
        let t5 = tables5();
        let bx = BitPerm::from_src(&cube_tables().ct_src);
        let bt = BitPerm::from_src(&t5.t_src);
        let ranks = Ranks::new();
        let all: Vec<u8> = (0..36).collect();
        let dir = table_dir();
        let ax = load_or_gen_quiet(&dir.join("stage555-x.bin"), 1 << 24, &|| gen_stage(&bx, GOAL_RL, &all));
        let at = load_or_gen_quiet(&dir.join("stage555-t.bin"), 1 << 24, &|| gen_stage(&bt, GOAL_RL, &all));
        // Phase-B moves keep the R/L centres on their faces, so the other 16
        // positions of each orbit are permuted among themselves.
        let table = |bp: &BitPerm| -> Vec<[u16; 22]> {
            (0..N_UD)
                .map(|r| {
                    let mut row = [0u16; 22];
                    for (k, &m) in B_MOVES.iter().enumerate() {
                        let y = bp.apply(m, ranks.unrank16[r] as u32);
                        assert!(y < 1 << 16, "phase-B moves keep the R/L centres in place");
                        row[k] = ranks.rank16[y as usize];
                    }
                    row
                })
                .collect()
        };
        let x_mv = table(&bx);
        let t_mv = table(&bt);
        let b = load_or_gen(&dir.join("phase2-555.bin"), N_B, || gen_b(&x_mv, &t_mv, &ranks));
        let g = &t5.geometry;
        let frames = [vec![], vec![0, 1], vec![0, 1, 0, 1]]
            .into_iter()
            .map(|turns| {
                let map = g.frame_moves(&turns);
                (turns, map)
            })
            .collect();
        let near = load_or_gen(&dir.join("stage555-near.bin"), BUCKETS / 2, || gen_near(&bx, &bt));
        Stage5 { bx, bt, ranks, ax, at, b, x_mv, t_mv, frames, near }
    }

    fn b_index(&self, c: &Cube5) -> usize {
        let x = self.ranks.rank16[ud_mask(&c.c4.ct) as usize] as usize;
        let t = self.ranks.rank16[ud_mask(&c.t) as usize] as usize;
        (x * N_UD + t) * 2 + c.c4.wing_parity() as usize
    }

    /// An optimal phase-B solution, by walking down the exact table.
    fn solve_b(&self, c: &Cube5) -> Vec<u8> {
        let mut idx = self.b_index(c);
        let mut out = Vec::new();
        while self.b[idx] > 0 {
            let (x, t, p) = (idx / 2 / N_UD, idx / 2 % N_UD, idx % 2);
            let next = B_MOVES.iter().enumerate().find_map(|(k, &m)| {
                let j = (self.x_mv[x][k] as usize * N_UD + self.t_mv[t][k] as usize) * 2 + (p ^ flips_parity(m) as usize);
                (self.b[j] + 1 == self.b[idx]).then_some((m, j))
            });
            let (m, j) = next.expect("the phase-B table is consistent");
            out.push(m);
            idx = j;
        }
        out
    }

    /// The shortest ways found to stage the centres of a cube given as 150
    /// facelets: phase A is enumerated up to `extra` moves past its optimum
    /// (at most `limit` solutions per frame and length), each completed
    /// optimally by phase B. Returns the best `keep`, shortest first.
    ///
    /// Past `deadline` the search stops as soon as it has a result: the optimal
    /// phase-A length is always completed, the extra lengths may be cut short.
    pub fn stage(
        &self,
        facelets: &[u8],
        extra: usize,
        limit: usize,
        keep: usize,
        deadline: Option<Instant>,
    ) -> Result<Vec<Staged>, String> {
        let g = &tables5().geometry;
        let mut roots = Vec::new();
        for (turns, map) in &self.frames {
            let cube = Cube5::from_facelets(&g.reframe(facelets, turns))?;
            roots.push((cube, map));
        }
        let h = |c: &Cube5| self.ax[rl_mask(&c.c4.ct) as usize].max(self.at[rl_mask(&c.t) as usize]) as usize;
        let first = roots.iter().map(|(c, _)| h(c)).min().unwrap();
        let mut all: Vec<(usize, Staged)> = Vec::new();
        let mut optimum = None;
        let stop = AtomicBool::new(false);
        for depth in first..=first + 12 {
            if optimum.is_some_and(|o| depth > o + extra) || stop.load(Relaxed) {
                break;
            }
            // Only the lengths past the optimum may be cut short.
            let cut = if optimum.is_some() { deadline } else { None };
            // One job per frame and first move.
            let jobs: Vec<(usize, u8)> =
                (0..roots.len()).flat_map(|f| (0..36u8).map(move |m| (f, m))).collect();
            let found: Vec<(usize, Staged)> = jobs
                .par_iter()
                .flat_map_iter(|&(f, m)| {
                    let (cube, map) = &roots[f];
                    let mut out = Vec::new();
                    if depth == 0 {
                        if m == 0 && h(cube) == 0 {
                            out.push(Vec::new());
                        }
                    } else {
                        let (x, t) = (self.bx.apply(m, rl_mask(&cube.c4.ct)), self.bt.apply(m, rl_mask(&cube.t)));
                        if self.ax[x as usize].max(self.at[t as usize]) as usize <= depth - 1 {
                            let mut path = [0u8; 32];
                            path[0] = m;
                            let mut ctx = Ctx { out: &mut out, limit, nodes: 0, stop: &stop, deadline: cut };
                            self.dfs_a(x, t, depth - 1, m, 1, &mut path, &mut ctx);
                        }
                    }
                    out.into_iter()
                        .map(|a| {
                            let mut c = *cube;
                            c.apply_seq(&a);
                            let b = self.solve_b(&c);
                            let moves: Vec<u8> = a.iter().chain(b.iter()).map(|&m| map[m as usize] as u8).collect();
                            (moves.len(), Staged { moves, len_a: a.len() })
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            if !found.is_empty() && optimum.is_none() {
                optimum = Some(depth);
            }
            all.extend(found);
        }
        all.sort_by_key(|s| s.0);
        all.truncate(keep);
        Ok(all.into_iter().map(|s| s.1).collect())
    }

    /// Phase A on the positions of the R/L centres alone (x orbit, t orbit),
    /// in this frame only: every solution of the shortest length and of the
    /// next `extra` lengths, at most `limit` per first move and length.
    /// For cubes whose centres move like these, such as the outer x centres
    /// and the oblique pairs of a 6x6x6.
    pub fn phase_a(&self, x: u32, t: u32, extra: usize, limit: usize, deadline: Option<Instant>) -> Vec<Vec<u8>> {
        let h = self.ax[x as usize].max(self.at[t as usize]) as usize;
        let stop = AtomicBool::new(false);
        let mut all = Vec::new();
        let mut optimum = None;
        for depth in h..=h + 12 {
            if optimum.is_some_and(|o| depth > o + extra) || stop.load(Relaxed) {
                break;
            }
            if depth == 0 {
                all.push(Vec::new());
                optimum = Some(0);
                continue;
            }
            let cut = if optimum.is_some() { deadline } else { None };
            let found: Vec<Vec<u8>> = (0..36u8)
                .into_par_iter()
                .flat_map_iter(|m| {
                    let mut out = Vec::new();
                    let (a, b) = (self.bx.apply(m, x), self.bt.apply(m, t));
                    if self.ax[a as usize].max(self.at[b as usize]) as usize <= depth - 1 {
                        let mut path = [0u8; 32];
                        path[0] = m;
                        let mut ctx = Ctx { out: &mut out, limit, nodes: 0, stop: &stop, deadline: cut };
                        self.dfs_a(a, b, depth - 1, m, 1, &mut path, &mut ctx);
                    }
                    out
                })
                .collect();
            if !found.is_empty() && optimum.is_none() {
                optimum = Some(depth);
            }
            all.extend(found);
        }
        all
    }

    /// An optimal phase B from the positions of the U/D centres among the
    /// sixteen places on U, D, F and B (x orbit, t orbit) and the parity of
    /// the wing permutation.
    pub fn phase_b(&self, x_ud: u32, t_ud: u32, odd: bool) -> Vec<u8> {
        let mut idx = (self.ranks.rank16[x_ud as usize] as usize * N_UD + self.ranks.rank16[t_ud as usize] as usize) * 2
            + odd as usize;
        let mut out = Vec::new();
        while self.b[idx] > 0 {
            let (x, t, p) = (idx / 2 / N_UD, idx / 2 % N_UD, idx % 2);
            let next = B_MOVES.iter().enumerate().find_map(|(k, &m)| {
                let j = (self.x_mv[x][k] as usize * N_UD + self.t_mv[t][k] as usize) * 2 + (p ^ flips_parity(m) as usize);
                (self.b[j] + 1 == self.b[idx]).then_some((m, j))
            });
            let (m, j) = next.expect("the phase-B table is consistent");
            out.push(m);
            idx = j;
        }
        out
    }

    fn dfs_a(&self, x: u32, t: u32, rem: usize, last: u8, depth: usize, path: &mut [u8; 32], ctx: &mut Ctx) -> bool {
        if rem == 0 {
            // The last move was checked to reach the goal.
            ctx.out.push(path[..depth].to_vec());
            return ctx.out.len() >= ctx.limit;
        }
        ctx.nodes += 1;
        if ctx.nodes % 4096 == 0 {
            if ctx.deadline.is_some_and(|d| Instant::now() > d) {
                ctx.stop.store(true, Relaxed);
            }
            if ctx.stop.load(Relaxed) {
                return true;
            }
        }
        for m in 0..36u8 {
            if skip_move(last, m) {
                continue;
            }
            let (a, b) = (self.bx.apply(m, x), self.bt.apply(m, t));
            if rem == 1 {
                if a != GOAL_RL || b != GOAL_RL {
                    continue;
                }
            } else if rem - 1 <= NEAR_A {
                let k = bucket(a, b);
                if ((self.near[k >> 1] >> ((k & 1) * 4)) & 15) as usize > rem - 1 {
                    continue;
                }
            } else if self.ax[a as usize].max(self.at[b as usize]) as usize > rem - 1 {
                continue;
            }
            // A sequence that ends with a phase-B move is a shorter one plus that move.
            if rem == 1 && B_MOVES.contains(&m) {
                continue;
            }
            path[depth] = m;
            if self.dfs_a(a, b, rem - 1, m, depth + 1, path, ctx) {
                return true;
            }
        }
        false
    }

    /// True if every centre is on a face of its own axis and the wing permutation is even.
    pub fn is_staged(c: &Cube5) -> bool {
        let on_axis = |col: &[u8; 24]| rl_mask(col) == GOAL_RL && ud_mask(col) == GOAL_UD;
        on_axis(&c.c4.ct) && on_axis(&c.t) && c.c4.wing_parity() == 0
    }
}

fn gen_b(x_mv: &[[u16; 22]], t_mv: &[[u16; 22]], ranks: &Ranks) -> Vec<u8> {
    let dist: Vec<AtomicU8> = (0..N_B).into_par_iter().map(|_| AtomicU8::new(UNREACHED)).collect();
    let goal = ranks.rank16[GOAL_UD as usize] as usize;
    dist[(goal * N_UD + goal) * 2].store(0, Relaxed);
    let mut depth = 0u8;
    let mut done = 1u64;
    loop {
        let new: u64 = (0..N_UD)
            .into_par_iter()
            .map(|x| {
                let mut n = 0u64;
                for t in 0..N_UD {
                    for p in 0..2 {
                        if dist[(x * N_UD + t) * 2 + p].load(Relaxed) != depth {
                            continue;
                        }
                        for (k, &m) in B_MOVES.iter().enumerate() {
                            let j = (x_mv[x][k] as usize * N_UD + t_mv[t][k] as usize) * 2
                                + (p ^ flips_parity(m) as usize);
                            if dist[j].load(Relaxed) == UNREACHED
                                && dist[j].compare_exchange(UNREACHED, depth + 1, Relaxed, Relaxed).is_ok()
                            {
                                n += 1;
                            }
                        }
                    }
                }
                n
            })
            .sum();
        if new == 0 {
            break;
        }
        depth += 1;
        done += new;
        eprintln!("  5x5x5 phase B table: depth {:2}  {:>10} new  {:>10} total", depth, new, done);
    }
    assert_eq!(done as usize, N_B, "every phase-B state must be reachable");
    let mut v = std::mem::ManuallyDrop::new(dist);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}
