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
use crate::sym::{axis_move_maps, closure, conjugators, destinations, renamed, CombRank, MaskMap};
use crate::tables::{gen_stage, load_or_gen, load_or_gen_quiet, table_dir, BitPerm, Bytes, Ranks, UNREACHED};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering::Relaxed};
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
    /// Moves to bring one orbit's R/L centres onto R and L, by the rank of
    /// their mask among the 735,471 there are: small enough to stay in the
    /// processor's cache, which the same table indexed by the mask is not.
    ax: Vec<u8>,
    at: Vec<u8>,
    rank8: CombRank,
    /// Exact length of phase B.
    b: Bytes,
    x_mv: Vec<[u16; 22]>,
    t_mv: Vec<[u16; 22]>,
    /// The three frames: whole-cube turns and the original name of each move.
    frames: Vec<(Vec<usize>, Vec<usize>)>,
    /// Phase-A lower bound near the goal; see `gen_near`.
    near: Bytes,
    /// The symmetries of phase A, as they move the x and the t positions.
    syms: Vec<(MaskMap, MaskMap)>,
}

/// Phase-A lower bound near the goal. Every state within `NEAR_A` moves of
/// the goal writes its exact distance into a bucket chosen by a hash; a
/// bucket keeps the smallest value written, and an untouched bucket means
/// "further than `NEAR_A`". Four bits per bucket, so most far states are
/// recognised as far. Never more than the true distance.
///
/// One move further out there are eighteen times as many states. They get
/// one bit each in a second part of the table: set for every state within
/// `NEAR_A + 1` moves, so a clear bit says "further than that".
///
/// States that are images of one another under the sixteen symmetries that
/// keep the R/L axis are equally far, so only one of each kind is stored
/// (`canonical`). Each move further the table reaches cuts the search
/// before it by a factor of about thirty.
const NEAR_A: usize = 7;
const NEAR_A_STATES: usize = 20_561_744;
/// States within six moves counted without the symmetries, as the table
/// that stored every one of them found: a check on the reduction.
const WITHIN_SIX: usize = 17_168_476;
const BUCKETS: usize = 1 << 27;
/// Bits of the part that reaches one move further.
const SHELL: usize = 1 << 31;
const NEAR_BYTES: usize = BUCKETS / 2 + SHELL / 8;

#[inline(always)]
fn bucket(key: u64) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 37) as usize & (BUCKETS - 1)
}

#[inline(always)]
fn shell_bit(key: u64) -> usize {
    (key.wrapping_mul(0xC2B2_AE3D_27D4_EB4F) >> 33) as usize & (SHELL - 1)
}

/// Every map of the x and t positions that commutes with the moves up to
/// renaming them and keeps the R/L faces in place.
fn a_symmetries() -> Vec<(MaskMap, MaskMap)> {
    let all: Vec<u8> = (0..36).collect();
    let dx = destinations(&cube_tables().ct_src);
    let dt = destinations(&tables5().t_src);
    let mut generators = Vec::new();
    for mu in axis_move_maps() {
        let pairs = renamed(&all, &mu);
        let xa = |m: usize, p: usize| dx[m][p] as usize;
        let ta = |m: usize, p: usize| dt[m][p] as usize;
        for px in conjugators(24, &pairs, &xa, &xa, &[]) {
            for pt in conjugators(24, &pairs, &ta, &ta, &[]) {
                if MaskMap::new(&px).apply(GOAL_RL) == GOAL_RL && MaskMap::new(&pt).apply(GOAL_RL) == GOAL_RL {
                    generators.push(vec![px.clone(), pt]);
                }
            }
        }
    }
    closure(&generators).into_iter().map(|e| (MaskMap::new(&e[0]), MaskMap::new(&e[1]))).collect()
}

/// The smallest image of a state under the symmetries, as one number.
#[inline(always)]
fn canonical(syms: &[(MaskMap, MaskMap)], x: u32, t: u32) -> u64 {
    let mut best = u64::MAX;
    for (sx, st) in syms {
        best = best.min((sx.apply(x) as u64) << 24 | st.apply(t) as u64);
    }
    best
}

fn gen_near(bx: &BitPerm, bt: &BitPerm, syms: &[(MaskMap, MaskMap)]) -> Vec<u8> {
    let far = (NEAR_A + 1) as u8;
    let mut table = vec![far | far << 4; BUCKETS / 2];
    let mut put = |key: u64, d: u8| {
        let b = bucket(key);
        let shift = (b & 1) * 4;
        if (table[b >> 1] >> shift) & 15 > d {
            table[b >> 1] = (table[b >> 1] & !(15 << shift)) | d << shift;
        }
    };
    let shell: Vec<AtomicU64> = (0..SHELL / 64).into_par_iter().map(|_| AtomicU64::new(0)).collect();
    let mark = |key: u64| {
        let b = shell_bit(key);
        let (cell, bit) = (&shell[b >> 6], 1u64 << (b & 63));
        if cell.load(Relaxed) & bit == 0 {
            cell.fetch_or(bit, Relaxed);
        }
    };
    // Level by level; a neighbour of a state is one level nearer, as near, or one further.
    let goal = canonical(syms, GOAL_RL, GOAL_RL);
    put(goal, 0);
    mark(goal);
    let (mut before, mut level) = (Vec::new(), vec![goal]);
    let mut states = 1;
    // How many states one stored state stands for.
    let images = |k: u64| {
        let (x, t) = ((k >> 24) as u32, (k & 0xFF_FFFF) as u32);
        let mut all: Vec<u64> = syms.iter().map(|(sx, st)| (sx.apply(x) as u64) << 24 | st.apply(t) as u64).collect();
        all.sort_unstable();
        all.dedup();
        all.len()
    };
    let neighbours = |k: u64| {
        let (x, t) = ((k >> 24) as u32, (k & 0xFF_FFFF) as u32);
        (0..36u8).map(move |m| canonical(syms, bx.apply(m, x), bt.apply(m, t)))
    };
    let mut within_six = 1;
    for depth in 1..=NEAR_A {
        let mut next: Vec<u64> = level.par_iter().flat_map_iter(|&k| neighbours(k)).collect();
        next.par_sort_unstable();
        next.dedup();
        next.retain(|k| before.binary_search(k).is_err() && level.binary_search(k).is_err());
        for &k in &next {
            put(k, depth as u8);
        }
        next.par_iter().for_each(|&k| mark(k));
        states += next.len();
        if depth <= 6 {
            within_six += next.par_iter().map(|&k| images(k)).sum::<usize>();
        }
        before = level;
        level = next;
    }
    assert_eq!(within_six, WITHIN_SIX, "states of phase A within six moves of the goal");
    assert_eq!(states, NEAR_A_STATES, "states of phase A within {} moves of the goal, up to symmetry", NEAR_A);
    // One move further: every neighbour of the last level. The ones that
    // are nearer have their bit already.
    level.par_iter().for_each(|&k| neighbours(k).for_each(|n| mark(n)));
    let set: u64 = shell.par_iter().map(|c| c.load(Relaxed).count_ones() as u64).sum();
    eprintln!("  5x5x5 phase A near table: {} states within {} moves, {:.1}% of the bits for {} moves set", states, NEAR_A, set as f64 * 100.0 / SHELL as f64, NEAR_A + 1);
    table.reserve(SHELL / 8);
    for c in shell {
        table.extend_from_slice(&c.into_inner().to_le_bytes());
    }
    table
}

/// Positions visited by phase A since the program started.
pub static POSITIONS: AtomicU64 = AtomicU64::new(0);

struct Ctx<'a> {
    out: &'a mut Vec<Vec<u8>>,
    limit: usize,
    nodes: u64,
    stop: &'a AtomicBool,
    deadline: Option<Instant>,
    /// With this, a phase-A solution is kept only if phase B after it is short enough.
    score: Option<Score<'a>>,
}

/// What it takes to know the length of phase B at the end of a phase-A
/// solution without making its moves on the whole cube: where the U/D
/// centres of both orbits are before it, and the parity of the wings.
struct Score<'a> {
    ud_x: u32,
    ud_t: u32,
    odd: bool,
    /// The shortest staging found so far, by any job.
    best: &'a AtomicUsize,
    /// Phase-A solutions seen, kept or not.
    seen: &'a AtomicUsize,
}

/// Stagings this much longer than the shortest one found are not kept:
/// the edge phases that follow cannot make up for more.
const SLACK: usize = 3;

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

/// The same over all 24 positions, for a cube whose R/L centres are not yet on their faces.
fn ud_mask_all(colours: &[u8; 24]) -> u32 {
    (0..24).fold(0, |m, i| m | ((colours[i] % 3 == 0) as u32) << i)
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
        let rank8 = CombRank::new(8);
        let by_rank = |file: &str, bp: &BitPerm| -> Vec<u8> {
            let by_mask = load_or_gen_quiet(&dir.join(file), 1 << 24, &|| gen_stage(bp, GOAL_RL, &all));
            rank8.unrank.iter().map(|&mask| by_mask[mask as usize]).collect()
        };
        let ax = by_rank("stage555-x.bin", &bx);
        let at = by_rank("stage555-t.bin", &bt);
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
        let syms = a_symmetries();
        assert_eq!(syms.len(), 16, "symmetries of phase A");
        let near = load_or_gen(&dir.join("stage555-near8.bin"), NEAR_BYTES, || gen_near(&bx, &bt, &syms));
        Stage5 { bx, bt, ranks, ax, at, rank8, b, x_mv, t_mv, frames, near, syms }
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
    /// (at most `limit` solutions kept per frame, first move and length),
    /// each completed optimally by phase B. Returns the best `keep`,
    /// shortest first.
    ///
    /// The length of phase B is read from its table at the end of every
    /// phase-A solution, from the positions of the U/D centres alone; only
    /// the solutions that give a short staging are worked out on the cube.
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
        let h = |c: &Cube5| self.a_bound(rl_mask(&c.c4.ct), rl_mask(&c.t)) as usize;
        let first = roots.iter().map(|(c, _)| h(c)).min().unwrap();
        // (length of the staging, frame, phase A)
        let mut all: Vec<(usize, usize, Vec<u8>)> = Vec::new();
        let mut optimum = None;
        let stop = AtomicBool::new(false);
        let best = AtomicUsize::new(usize::MAX);
        for depth in first..=first + 12 {
            if optimum.is_some_and(|o| depth > o + extra) || stop.load(Relaxed) {
                break;
            }
            // Only the lengths past the optimum may be cut short.
            let cut = if optimum.is_some() { deadline } else { None };
            let seen = AtomicUsize::new(0);
            // One job per frame and first move.
            let jobs: Vec<(usize, u8)> =
                (0..roots.len()).flat_map(|f| (0..36u8).map(move |m| (f, m))).collect();
            let found: Vec<(usize, usize, Vec<u8>)> = jobs
                .par_iter()
                .flat_map_iter(|&(f, m)| {
                    let cube = &roots[f].0;
                    let mut out = Vec::new();
                    let score = || Score {
                        ud_x: ud_mask_all(&cube.c4.ct),
                        ud_t: ud_mask_all(&cube.t),
                        odd: cube.c4.wing_parity() != 0,
                        best: &best,
                        seen: &seen,
                    };
                    let mut ctx = Ctx { out: &mut out, limit, nodes: 0, stop: &stop, deadline: cut, score: Some(score()) };
                    if depth == 0 {
                        if m == 0 && h(cube) == 0 {
                            self.leaf(&[], &mut ctx);
                        }
                    } else {
                        let (x, t) = (self.bx.apply(m, rl_mask(&cube.c4.ct)), self.bt.apply(m, rl_mask(&cube.t)));
                        if self.a_bound(x, t) as usize <= depth - 1 {
                            let mut path = [0u8; 32];
                            path[0] = m;
                            self.dfs_a(x, t, depth - 1, m, 1, &mut path, &mut ctx);
                        }
                    }
                    POSITIONS.fetch_add(ctx.nodes, Relaxed);
                    let score = score();
                    out.into_iter().map(|a| (a.len() + self.b_after(&score, &a), f, a)).collect::<Vec<_>>()
                })
                .collect();
            if seen.load(Relaxed) > 0 && optimum.is_none() {
                optimum = Some(depth);
            }
            all.extend(found);
        }
        // Among stagings of one length, no frame or first move comes first.
        let mix = |a: &[u8]| a.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, &m| (h ^ m as u64).wrapping_mul(0x100_0000_01B3));
        all.sort_by_cached_key(|s| (s.0, mix(&s.2), s.1));
        all.truncate(keep);
        Ok(all
            .into_par_iter()
            .map(|(len, f, a)| {
                let (cube, map) = &roots[f];
                let mut c = *cube;
                c.apply_seq(&a);
                let b = self.solve_b(&c);
                let moves: Vec<u8> = a.iter().chain(b.iter()).map(|&m| map[m as usize] as u8).collect();
                assert_eq!(moves.len(), len, "phase B is as long as its table said");
                Staged { moves, len_a: a.len() }
            })
            .collect())
    }

    /// The length of phase B after a phase-A solution.
    fn b_after(&self, score: &Score, a: &[u8]) -> usize {
        let (mut x, mut t, mut odd) = (score.ud_x, score.ud_t, score.odd);
        for &m in a {
            x = self.bx.apply(m, x);
            t = self.bt.apply(m, t);
            odd ^= flips_parity(m);
        }
        debug_assert!(x < 1 << 16 && t < 1 << 16, "phase A leaves the U/D centres off the R and L faces");
        self.b_dist(self.ranks.rank16[x as usize] as usize, self.ranks.rank16[t as usize] as usize, odd) as usize
    }

    /// A phase-A solution has been found; true if the search should end.
    fn leaf(&self, a: &[u8], ctx: &mut Ctx) -> bool {
        if let Some(score) = &ctx.score {
            score.seen.fetch_add(1, Relaxed);
            let total = a.len() + self.b_after(score, a);
            let best = score.best.fetch_min(total, Relaxed).min(total);
            if total > best + SLACK {
                return false;
            }
        }
        ctx.out.push(a.to_vec());
        ctx.out.len() >= ctx.limit
    }

    /// Phase A on the positions of the R/L centres alone (x orbit, t orbit),
    /// in this frame only: every solution of the shortest length and of the
    /// next `extra` lengths, at most `limit` per first move and length.
    /// For cubes whose centres move like these, such as the outer x centres
    /// and the oblique pairs of a 6x6x6.
    pub fn phase_a(&self, x: u32, t: u32, extra: usize, limit: usize, deadline: Option<Instant>) -> Vec<Vec<u8>> {
        let h = self.a_bound(x, t) as usize;
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
                    if self.a_bound(a, b) as usize <= depth - 1 {
                        let mut path = [0u8; 32];
                        path[0] = m;
                        let mut ctx = Ctx { out: &mut out, limit, nodes: 0, stop: &stop, deadline: cut, score: None };
                        self.dfs_a(a, b, depth - 1, m, 1, &mut path, &mut ctx);
                        POSITIONS.fetch_add(ctx.nodes, Relaxed);
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

    /// A lower bound on phase A from the positions of the R/L centres of the two orbits.
    #[inline(always)]
    pub fn a_bound(&self, x: u32, t: u32) -> u8 {
        self.ax[self.rank8.rank(x)].max(self.at[self.rank8.rank(t)])
    }

    /// Exact length of phase B from the ranks (`Ranks::rank16`) of the U/D
    /// patterns of the x and t orbits and the parity of the wing permutation.
    /// For searches on larger cubes that do the work of phase B among other things.
    #[inline(always)]
    pub fn b_dist(&self, x: usize, t: usize, odd: bool) -> u8 {
        self.b[(x * N_UD + t) * 2 + odd as usize]
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

    /// All the children of a position are looked at together, one table
    /// at a time: the entries are far apart in memory, and read this way
    /// they are fetched side by side.
    fn dfs_a(&self, x: u32, t: u32, rem: usize, last: u8, depth: usize, path: &mut [u8; 32], ctx: &mut Ctx) -> bool {
        if rem == 0 {
            // The last move was checked to reach the goal.
            return self.leaf(&path[..depth], ctx);
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
        if rem == 1 {
            for m in 0..36u8 {
                // A sequence that ends with a phase-B move is a shorter one plus that move.
                if skip_move(last, m) || B_MOVES.contains(&m) {
                    continue;
                }
                if self.bx.apply(m, x) == GOAL_RL && self.bt.apply(m, t) == GOAL_RL {
                    path[depth] = m;
                    if self.leaf(&path[..depth + 1], ctx) {
                        return true;
                    }
                }
            }
            return false;
        }
        let max = (rem - 1) as u8;
        let mut kids = [(0u32, 0u32); 36];
        let mut mv = [0u8; 36];
        let mut cnt = 0;
        for m in 0..36u8 {
            if skip_move(last, m) {
                continue;
            }
            kids[cnt] = (self.bx.apply(m, x), self.bt.apply(m, t));
            mv[cnt] = m;
            cnt += 1;
        }
        // Each orbit alone.
        let mut h = [0u8; 36];
        for i in 0..cnt {
            h[i] = self.a_bound(kids[i].0, kids[i].1);
        }
        let mut live = 0;
        for i in 0..cnt {
            if h[i] <= max {
                kids[live] = kids[i];
                mv[live] = mv[i];
                live += 1;
            }
        }
        // Both orbits together, near the goal: the distance itself, or one
        // move further out whether the goal is within reach at all.
        if rem - 1 <= NEAR_A + 1 {
            let exact = rem - 1 <= NEAR_A;
            let mut at = [0usize; 36];
            for i in 0..live {
                let key = canonical(&self.syms, kids[i].0, kids[i].1);
                at[i] = if exact { bucket(key) } else { shell_bit(key) };
            }
            let mut ok = [false; 36];
            for i in 0..live {
                ok[i] = if exact {
                    (self.near[at[i] >> 1] >> ((at[i] & 1) * 4)) & 15 <= max
                } else {
                    (self.near[BUCKETS / 2 + (at[i] >> 3)] >> (at[i] & 7)) & 1 == 1
                };
            }
            let all = live;
            live = 0;
            for i in 0..all {
                if ok[i] {
                    kids[live] = kids[i];
                    mv[live] = mv[i];
                    live += 1;
                }
            }
        }
        for i in 0..live {
            path[depth] = mv[i];
            if self.dfs_a(kids[i].0, kids[i].1, rem - 1, mv[i], depth + 1, path, ctx) {
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
