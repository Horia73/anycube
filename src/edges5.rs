//! 5x5x5, second part: from staged centres to a reduced cube.
//!
//! Wing positions `s` and `s + 12` and middle-edge slot `s` form edge `s`.
//! A wing piece `j` has class `j % 12` (its two colours) and type `j / 12`;
//! positions 0..12 are the "low" half, 12..24 the "high" half.
//!
//! Phase C (face turns and wide half turns) makes the edges ready to be
//! paired by moves that never mix the halves: every class gets one wing in
//! each half, and its low wing agrees with the flip of its middle edge.
//! Phase P (F and B freely, half turns elsewhere) builds four whole edges in
//! the middle slice, phase E (U and D freely) the other eight.
//!
//! Every phase starts from many cubes at once and keeps the continuations
//! that are shortest in total, so a longer step is taken where it pays later.

use crate::c3::{perm_rank, perm_unrank, T3};
use crate::c5::{tables5, Cube5};
use crate::cube::{skip_move, tables as cube_tables};
use crate::sym::{axis_move_maps, closure, conjugators, destinations, renamed, MaskMap};
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

/// Face turns and wide half turns: everything that keeps the centres staged.
pub const H_MOVES: [u8; 24] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 22, 25, 28, 31, 34];

const N_MASK: usize = 2_704_156; // 12 of 24
const N_FLIP: usize = 2048;
const LOW_HALF: u32 = 0x000FFF;
const UNSEEN: u8 = 15;

/// Rank of a 12-of-24 mask (colex), byte by byte, and the inverse.
pub struct MaskRank {
    tab: Vec<[[u32; 13]; 256]>,
    pub unrank: Vec<u32>,
}

impl MaskRank {
    pub fn new() -> MaskRank {
        let mut binom = [[0u32; 14]; 25];
        for n in 0..25 {
            binom[n][0] = 1;
            for k in 1..14.min(n + 1) {
                binom[n][k] = binom[n - 1][k - 1] + if k < n { binom[n - 1][k] } else { 0 };
            }
        }
        let mut tab = vec![[[0u32; 13]; 256]; 3];
        for k in 0..3 {
            for byte in 0..256usize {
                for below in 0..13usize {
                    let (mut r, mut i) = (0, below);
                    for j in 0..8 {
                        if (byte >> j) & 1 == 1 {
                            i += 1;
                            if i <= 13 {
                                r += binom[8 * k + j][i];
                            }
                        }
                    }
                    tab[k][byte][below] = r;
                }
            }
        }
        let unrank: Vec<u32> = (0u32..1 << 24).filter(|x| x.count_ones() == 12).collect();
        assert_eq!(unrank.len(), N_MASK);
        let r = MaskRank { tab, unrank };
        for &i in &[0usize, 1, 99_999, N_MASK - 1] {
            assert_eq!(r.rank(r.unrank[i]), i);
        }
        r
    }

    #[inline(always)]
    pub fn rank(&self, m: u32) -> usize {
        let (b0, b1, b2) = ((m & 255) as usize, ((m >> 8) & 255) as usize, (m >> 16) as usize);
        let c0 = b0.count_ones() as usize;
        let c1 = c0 + b1.count_ones() as usize;
        (self.tab[0][b0][0] + self.tab[1][b1][c0.min(12)] + self.tab[2][b2][c1.min(12)]) as usize
    }
}

impl Default for MaskRank {
    fn default() -> MaskRank {
        MaskRank::new()
    }
}

#[inline(always)]
fn nib(bytes: &[AtomicU8], idx: usize) -> u8 {
    (bytes[idx >> 1].load(Relaxed) >> ((idx & 1) * 4)) & 15
}

/// Sets an unseen entry to `v`; returns true if this call did it.
#[inline(always)]
fn nib_set(bytes: &[AtomicU8], idx: usize, v: u8) -> bool {
    let shift = (idx & 1) * 4;
    let cell = &bytes[idx >> 1];
    let mut cur = cell.load(Relaxed);
    loop {
        if (cur >> shift) & 15 != UNSEEN {
            return false;
        }
        match cell.compare_exchange_weak(cur, (cur & !(15 << shift)) | v << shift, Relaxed, Relaxed) {
            Ok(_) => return true,
            Err(now) => cur = now,
        }
    }
}

/// The symmetries of phase C: for each, where a set of wing positions goes
/// (a permutation, then perhaps the complement) and where a flip coordinate goes.
struct CSym {
    wings: MaskMap,
    complement: bool,
    flips: Vec<u16>,
}

impl CSym {
    #[inline(always)]
    fn mask(&self, mask: u32) -> u32 {
        let x = self.wings.apply(mask);
        if self.complement {
            !x & 0xFF_FFFF
        } else {
            x
        }
    }
}

/// Every map of (wing mask, flip) that commutes with the moves of phase C
/// up to renaming them and keeps the goal in place. Besides the sixteen
/// symmetries of the cube that keep the R/L axis, the two wings of every
/// edge can change places.
fn c_symmetries(flip_mv: &[[u16; 18]]) -> Vec<CSym> {
    let dest = destinations(&cube_tables().ep_src);
    let mut generators = Vec::new();
    for mu in axis_move_maps() {
        let pairs = renamed(&H_MOVES, &mu);
        let flip = |m: usize, f: usize| flip_mv[f][m % 18] as usize;
        let flips = conjugators(N_FLIP, &pairs, &flip, &flip, &[(0, 0)]);
        let Some(flips) = flips.into_iter().next() else { continue };
        let wing = |m: usize, p: usize| dest[m][p] as usize;
        for wings in conjugators(24, &pairs, &wing, &wing, &[]) {
            let image = MaskMap::new(&wings).apply(LOW_HALF);
            let complement = match image {
                LOW_HALF => vec![0, 1],
                x if x == !LOW_HALF & 0xFF_FFFF => vec![1, 0],
                _ => continue,
            };
            generators.push(vec![wings, flips.clone(), complement]);
        }
    }
    closure(&generators)
        .into_iter()
        .map(|e| CSym {
            wings: MaskMap::new(&e[0]),
            complement: e[2][0] == 1,
            flips: e[1].iter().map(|&f| f as u16).collect(),
        })
        .collect()
}

/// For every wing mask, by rank: its class and the symmetry that takes it to
/// the class's representative, as `class << 6 | symmetry`.
fn c_classes(syms: &[CSym], rk: &MaskRank) -> Vec<u32> {
    let best: Vec<(u32, u32)> = (0..N_MASK)
        .into_par_iter()
        .map(|r| {
            let mask = rk.unrank[r];
            syms.iter().enumerate().map(|(s, sym)| (sym.mask(mask), s as u32)).min().unwrap()
        })
        .collect();
    let mut reps: Vec<u32> = best.iter().map(|b| b.0).collect();
    reps.sort_unstable();
    reps.dedup();
    best.par_iter().map(|&(rep, s)| (reps.binary_search(&rep).unwrap() as u32) << 6 | s).collect()
}

/// Phase C: moves until the low-labelled wings fill the low half and the
/// (labelled) middle edges are unflipped, for every (class of wing masks,
/// flip) pair. Four bits per entry; 15 stands for the few states that need
/// 15 moves.
fn gen_c_sym(bw: &BitPerm, flip_mv: &[[u16; 18]], rk: &MaskRank, syms: &[CSym], cls: &[u32]) -> Vec<u8> {
    let n_cls = cls.iter().map(|&c| c >> 6).max().unwrap() as usize + 1;
    // Representative of each class and the symmetries that keep it in place.
    let mut reps = vec![0u32; n_cls];
    for (r, &c) in cls.iter().enumerate() {
        reps[(c >> 6) as usize] = syms[(c & 63) as usize].mask(rk.unrank[r]);
    }
    let stab: Vec<u64> = reps
        .par_iter()
        .map(|&x| syms.iter().enumerate().filter(|(_, s)| s.mask(x) == x).fold(0u64, |a, (i, _)| a | 1 << i))
        .collect();
    let bytes: Vec<AtomicU8> = (0..n_cls * N_FLIP / 2).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    // Sets a state and its images under the symmetries that keep its class representative.
    let set = |c: usize, f: usize, v: u8| -> u64 {
        let mut n = 0;
        let mut st = stab[c];
        while st != 0 {
            let s = st.trailing_zeros() as usize;
            st &= st - 1;
            n += nib_set(&bytes, c * N_FLIP + syms[s].flips[f] as usize, v) as u64;
        }
        n
    };
    let goal = cls[rk.rank(LOW_HALF)];
    set((goal >> 6) as usize, syms[(goal & 63) as usize].flips[0] as usize, 0);
    let mut done = 1u64;
    for depth in 0..14u8 {
        let n: u64 = (0..n_cls)
            .into_par_iter()
            .with_min_len(16)
            .map(|c| {
                let mut new = 0;
                for f in 0..N_FLIP {
                    if nib(&bytes, c * N_FLIP + f) != depth {
                        continue;
                    }
                    for &m in &H_MOVES {
                        let to = cls[rk.rank(bw.apply(m, reps[c]))];
                        let g = syms[(to & 63) as usize].flips[flip_mv[f][m as usize % 18] as usize] as usize;
                        new += set((to >> 6) as usize, g, depth + 1);
                    }
                }
                new
            })
            .sum();
        done += n;
        eprintln!("  5x5x5 phase C table: depth {:2}  {:>10} new  {:>10} total", depth + 1, n, done);
    }
    eprintln!("  5x5x5 phase C table: {} classes, {} entries further than 14 moves", n_cls, (n_cls * N_FLIP) as u64 - done);
    let mut v = std::mem::ManuallyDrop::new(bytes);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

/// Moves that never mix the wing halves and never flip a middle edge:
/// U D F B freely, R2 L2, and the six wide half turns.
pub const G_MOVES: [u8; 20] = [0, 1, 2, 6, 7, 8, 9, 10, 11, 15, 16, 17, 4, 13, 19, 22, 25, 28, 31, 34];
/// Moves that keep the pieces of the middle slice in the middle slice:
/// U D freely, the other face half turns, and the six wide half turns.
/// The first 14 never touch the pairing of the middle-slice edges.
pub const E_MOVES: [u8; 16] = [0, 1, 2, 9, 10, 11, 4, 7, 13, 16, 22, 25, 31, 34, 19, 28];

/// Phase P: F and B freely, the other face half turns, the six wide half turns.
/// Pieces of the slice between F and B stay in it; the other eight edges
/// (`OUTER_P`) are where four whole edges get built, in the middle slice.
pub const P_MOVES: [u8; 16] = [6, 7, 8, 15, 16, 17, 1, 10, 4, 13, 19, 22, 25, 28, 31, 34];
const OUTER_P: [usize; 8] = [0, 2, 4, 6, 8, 9, 10, 11];
/// Middle edges of four classes with their low (or high) wings.
const N_MW: usize = 70 * 1680;
const N_Q: usize = N_MW * 1680;

const N_P8: usize = 40320;
const N_E: usize = N_P8 * N_P8;
/// Centres as they are when solved: bit set where a centre shows U, F or R.
const SOLVED_CENTRES: u32 = 0x0F0F0F;
const FAR: u8 = 255;

/// Where the pieces of each kind go under a move, as slot permutations.
#[derive(Clone, Copy)]
struct SlotMove {
    mid: [u8; 12],
    low: [u8; 12],
    high: [u8; 12],
}

pub struct Edges5 {
    pub t3: T3,
    /// Mask permutations of the wing positions and of the x and t centres.
    pub bw: BitPerm,
    bx: BitPerm,
    bt: BitPerm,
    pub rk: MaskRank,
    rank8: [u8; 256],
    /// Phase C: the table over (class of wing masks, flip), the class and
    /// symmetry of every wing mask, and the flip maps of the symmetries.
    c: Bytes,
    c_cls: Vec<u32>,
    c_phi: Vec<Vec<u16>>,
    /// Phase C centres: moves until the R-vs-L patterns on the R and L faces
    /// are ones the later phases can finish, by (x pattern, t pattern).
    c_centres: Vec<u8>,
    /// Phase E, outer edges: pairing permutations, their moves, and the exact
    /// pairing table (modulo 3).
    out_low_mv: Vec<[u16; 14]>,
    out_high_mv: Vec<[u16; 14]>,
    e: Bytes,
    /// Centres of the U and D faces (x pattern, t pattern) and their moves.
    ud_mv: Vec<[u16; 16]>,
    /// Centres of the four side faces: the patterns phase E can reach, numbered.
    side_id: std::collections::HashMap<u32, u32>,
    /// Phase P: slot moves among the eight outer edges, ordered 4-tuples of
    /// them, the exact table for building four edges in the middle slice,
    /// and the centre table (F/B pattern, R/L pattern).
    outer_index: [u8; 12],
    rank1680: Vec<u16>,
    ml_mv: Vec<[u32; 16]>,
    mh_mv: Vec<[u32; 16]>,
    pq: Bytes,
    fb_mv: Vec<[u16; 16]>,
    rl_mv: Vec<[u16; 16]>,
    p_centres: Bytes,
    /// Phase P, edges against centres: middle edges with low (0) or high (1)
    /// wings, with the F/B pattern and with the R/L pattern (numbered by `rl_id`).
    p_fb: [Bytes; 2],
    p_rl: [Bytes; 2],
    rl_id: Vec<u16>,
    n_rl: usize,
    /// Phase E, edges against centres: low (0) or high (1) pairing with the
    /// U/D pattern and with the side pattern.
    e_ud: [Bytes; 2],
    e_side: [Vec<u8>; 2],
    /// The side patterns phase E can finish, numbered, with their moves in it.
    side_last: Vec<u8>,
    last_mv: Vec<[u8; 14]>,
    /// Phase E centres: exact moves to solve (U/D pattern, side pattern).
    e_centres: Vec<u8>,
}

/// Coordinates of four classes among the eight outer edges, whose middle
/// edges, low wings and high wings sit at positions `m[i]`, `l[i]`, `h[i]`:
/// (middle edges with low wings, middle edges with high wings), each below
/// `N_MW`. The classes are unordered, so they are sorted by the middle
/// edge's position.
#[inline(always)]
fn q_parts(rank8: &[u8; 256], rank1680: &[u16], m: &[u8; 4], l: &[u8; 4], h: &[u8; 4]) -> (usize, usize) {
    let mut order = [0usize, 1, 2, 3];
    order.sort_unstable_by_key(|&i| m[i]);
    let mask = (1usize << m[0]) | (1 << m[1]) | (1 << m[2]) | (1 << m[3]);
    let code = |t: &[u8; 4]| {
        t[order[0]] as usize | (t[order[1]] as usize) << 3 | (t[order[2]] as usize) << 6 | (t[order[3]] as usize) << 9
    };
    let set = rank8[mask] as usize * 1680;
    (set + rank1680[code(l)] as usize, set + rank1680[code(h)] as usize)
}

/// Index into the exact phase-P edge table.
#[inline(always)]
fn q_index(rank8: &[u8; 256], rank1680: &[u16], m: &[u8; 4], l: &[u8; 4], h: &[u8; 4]) -> usize {
    let (ml, mh) = q_parts(rank8, rank1680, m, l, h);
    ml * 1680 + mh % 1680
}

/// Exact distances to a set of goals for a pair of coordinates, as nibbles;
/// 15 stands for 15 moves or more, or unreachable. `next_a(a, k)` and
/// `next_b(b, k)` are the coordinates after move `k`; the moves must be
/// closed under inversion.
fn gen_pair(
    name: &str,
    (na, nb, moves): (usize, usize, usize),
    next_a: impl Fn(usize, usize) -> usize + Sync,
    next_b: impl Fn(usize, usize) -> usize + Sync,
    goals: &[(usize, usize)],
) -> Vec<u8> {
    let bytes: Vec<AtomicU8> = (0..(na * nb).div_ceil(2)).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    for &(a, b) in goals {
        nib_set(&bytes, a * nb + b, 0);
    }
    let mut done = goals.len() as u64;
    for depth in 0..14u8 {
        let n: u64 = (0..na)
            .into_par_iter()
            .map(|a| {
                let mut new = 0;
                for b in 0..nb {
                    if nib(&bytes, a * nb + b) == depth {
                        for k in 0..moves {
                            if nib_set(&bytes, next_a(a, k) * nb + next_b(b, k), depth + 1) {
                                new += 1;
                            }
                        }
                    }
                }
                new
            })
            .sum();
        done += n;
        if n == 0 {
            break;
        }
    }
    eprintln!("  5x5x5 {} table: {} of {} states within 14 moves", name, done, na * nb);
    let mut v = std::mem::ManuallyDrop::new(bytes);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

#[inline(always)]
fn nibble(bytes: &[u8], idx: usize) -> u8 {
    (bytes[idx >> 1] >> ((idx & 1) * 4)) & 15
}

/// Bit set where a centre shows the first colour of its axis (U, F or R).
fn primary(colours: &[u8; 24]) -> u32 {
    (0..24).fold(0, |m, i| m | ((colours[i] < 3) as u32) << i)
}

impl Edges5 {
    pub fn load() -> Edges5 {
        let t3 = T3::new();
        let ct = cube_tables();
        let bw = BitPerm::from_src(&ct.ep_src);
        let bx = BitPerm::from_src(&ct.ct_src);
        let bt = BitPerm::from_src(&tables5().t_src);
        let rk = MaskRank::new();
        let dir = table_dir();
        let c_syms = c_symmetries(&t3.flip_mv);
        let c_cls = load_or_gen(&dir.join("phase3-555-classes.bin"), N_MASK * 4, || {
            c_classes(&c_syms, &rk).iter().flat_map(|x| x.to_le_bytes()).collect()
        });
        let c_cls: Vec<u32> = c_cls.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect();
        let n_cls = c_cls.iter().map(|&c| c >> 6).max().unwrap() as usize + 1;
        let c = load_or_gen(&dir.join("phase3-555-sym.bin"), n_cls * N_FLIP / 2, || {
            eprintln!("  5x5x5 phase C: {} symmetries", c_syms.len());
            gen_c_sym(&bw, &t3.flip_mv, &rk, &c_syms, &c_cls)
        });
        let c_phi: Vec<Vec<u16>> = c_syms.into_iter().map(|s| s.flips).collect();
        let mut rank8 = [255u8; 256];
        let mut n = 0;
        for x in 0..256usize {
            if x.count_ones() == 4 {
                rank8[x] = n;
                n += 1;
            }
        }

        let slot_move = |m: u8| -> SlotMove {
            let mut sm = SlotMove { mid: [0; 12], low: [0; 12], high: [0; 12] };
            let mid = &t3.moves[m as usize % 18];
            let src = &ct.ep_src[m as usize];
            for s in 0..12 {
                sm.mid[mid.ep[s] as usize] = s as u8;
                assert!(src[s] < 12 && src[s + 12] >= 12, "these moves keep the wing halves");
                sm.low[src[s] as usize] = s as u8;
                sm.high[(src[s + 12] - 12) as usize] = s as u8;
            }
            sm
        };
        let e_slots: Vec<SlotMove> = E_MOVES.iter().map(|&m| slot_move(m)).collect();
        for sm in &e_slots {
            for s in 0..12 {
                let outer = s < 8;
                assert!(
                    (sm.mid[s] < 8) == outer && (sm.low[s] < 8) == outer && (sm.high[s] < 8) == outer,
                    "these moves keep the middle slice apart"
                );
            }
        }

        // Phase C centres.
        let rl = |bp: &BitPerm, m: u8, pat: usize| (bp.apply(m, (pat as u32) << 16) >> 16) as usize;
        let cc_index = |x: usize, t: usize| rank8[x] as usize * 70 + rank8[t] as usize;
        let mut ready = vec![false; 4900];
        let mut stack = vec![(0x0Fusize, 0x0Fusize)];
        ready[cc_index(0x0F, 0x0F)] = true;
        while let Some((x, t)) = stack.pop() {
            for &m in &G_MOVES {
                let (a, b) = (rl(&bx, m, x), rl(&bt, m, t));
                if !ready[cc_index(a, b)] {
                    ready[cc_index(a, b)] = true;
                    stack.push((a, b));
                }
            }
        }
        let mut c_centres = vec![FAR; 4900];
        let mut frontier = Vec::new();
        for x in 0..256usize {
            for t in 0..256usize {
                if rank8[x] != 255 && rank8[t] != 255 && ready[cc_index(x, t)] {
                    c_centres[cc_index(x, t)] = 0;
                    frontier.push((x, t));
                }
            }
        }
        let mut depth = 0;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &(x, t) in &frontier {
                for &m in &H_MOVES {
                    let (a, b) = (rl(&bx, m, x), rl(&bt, m, t));
                    if c_centres[cc_index(a, b)] == FAR {
                        c_centres[cc_index(a, b)] = depth + 1;
                        next.push((a, b));
                    }
                }
            }
            frontier = next;
            depth += 1;
        }

        // Phase E centres. U/D faces: any pattern of the two orbits.
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        let ud_mv: Vec<[u16; 16]> = (0..4900)
            .map(|i| {
                let mut row = [0u16; 16];
                for (k, &m) in E_MOVES.iter().enumerate() {
                    let (a, b) = (bx.apply(m, unrank8[i / 70]), bt.apply(m, unrank8[i % 70]));
                    assert!(a < 256 && b < 256, "these moves keep the U/D centres on U and D");
                    row[k] = rank8[a as usize] as u16 * 70 + rank8[b as usize] as u16;
                }
                row
            })
            .collect();
        // Side faces: the patterns reachable from solved, numbered as found.
        let side_key = |x: u32, t: u32| (x >> 8) << 16 | (t >> 8);
        let mut side_id = std::collections::HashMap::new();
        let mut sides: Vec<(u32, u32)> = vec![(SOLVED_CENTRES & !0xFF, SOLVED_CENTRES & !0xFF)];
        side_id.insert(side_key(sides[0].0, sides[0].1), 0u32);
        let mut side_mv: Vec<[u32; 16]> = Vec::new();
        let mut i = 0;
        while i < sides.len() {
            let (x, t) = sides[i];
            let mut row = [0u32; 16];
            for (k, &m) in E_MOVES.iter().enumerate() {
                let (a, b) = (bx.apply(m, x), bt.apply(m, t));
                assert!(a & 0xFF == 0 && b & 0xFF == 0, "these moves keep the side centres on the sides");
                let next = sides.len() as u32;
                let id = *side_id.entry(side_key(a, b)).or_insert(next);
                if id == next {
                    sides.push((a, b));
                }
                row[k] = id;
            }
            side_mv.push(row);
            i += 1;
        }
        let n_side = sides.len();
        assert_eq!(n_side, 186_624);
        // Phase E, outer edges.
        let perm_table = |high: bool| -> Vec<[u16; 14]> {
            (0..N_P8)
                .map(|r| {
                    let mut sigma = [0u8; 8];
                    perm_unrank(r, &mut sigma);
                    let mut row = [0u16; 14];
                    for (k, sm) in e_slots[..14].iter().enumerate() {
                        // The wing at slot `s` goes to `wing[s]`; its middle edge moves too.
                        let wing = if high { &sm.high } else { &sm.low };
                        let mut out = [0u8; 8];
                        for s in 0..8 {
                            out[wing[s] as usize] = sm.mid[sigma[s] as usize];
                        }
                        row[k] = perm_rank(&out) as u16;
                    }
                    row
                })
                .collect()
        };
        let out_low_mv = perm_table(false);
        let out_high_mv = perm_table(true);
        let e = load_or_gen(&dir.join("phase5-555.bin"), N_E.div_ceil(4), || gen_e(&out_low_mv, &out_high_mv));
        // The side patterns the 14 moves of the last phase reach from solved.
        let mut side_last = vec![FAR; n_side];
        let mut last_ids = vec![0usize];
        side_last[0] = 0;
        let mut i = 0;
        while i < last_ids.len() {
            for k in 0..14 {
                let y = side_mv[last_ids[i]][k] as usize;
                if side_last[y] == FAR {
                    side_last[y] = last_ids.len() as u8;
                    last_ids.push(y);
                }
            }
            i += 1;
        }
        let n_last = last_ids.len();
        assert_eq!(n_last, 36);
        let last_mv: Vec<[u8; 14]> = last_ids
            .iter()
            .map(|&id| {
                let mut row = [0u8; 14];
                for k in 0..14 {
                    row[k] = side_last[side_mv[id][k] as usize];
                }
                row
            })
            .collect();
        // Phase E centres: (U/D pattern, side pattern) under the 14 moves.
        let ud_solved = rank8[0x0F] as usize * 70 + rank8[0x0F] as usize;
        let mut e_centres = vec![FAR; 4900 * n_last];
        e_centres[ud_solved * n_last] = 0;
        let mut frontier = vec![ud_solved * n_last];
        let mut depth = 0u8;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &x in &frontier {
                let (ud, sd) = (x / n_last, x % n_last);
                for k in 0..14 {
                    let y = ud_mv[ud][k] as usize * n_last + last_mv[sd][k] as usize;
                    if e_centres[y] == FAR {
                        e_centres[y] = depth + 1;
                        next.push(y);
                    }
                }
            }
            frontier = next;
            depth += 1;
        }

        // Phase P.
        let mut outer_index = [255u8; 12];
        for (i, &s) in OUTER_P.iter().enumerate() {
            outer_index[s] = i as u8;
        }
        let p_perm: Vec<[[u8; 8]; 3]> = P_MOVES
            .iter()
            .map(|&m| {
                let sm = slot_move(m);
                let mut row = [[0u8; 8]; 3];
                for (i, &s) in OUTER_P.iter().enumerate() {
                    row[0][i] = outer_index[sm.mid[s] as usize];
                    row[1][i] = outer_index[sm.low[s] as usize];
                    row[2][i] = outer_index[sm.high[s] as usize];
                    assert!(row[0][i] < 8 && row[1][i] < 8 && row[2][i] < 8, "these moves keep the F/B slice apart");
                }
                row
            })
            .collect();
        let mut rank1680 = vec![u16::MAX; 4096];
        let mut unrank1680: Vec<[u8; 4]> = Vec::new();
        for code in 0..4096usize {
            let t = [(code & 7) as u8, (code >> 3 & 7) as u8, (code >> 6 & 7) as u8, (code >> 9) as u8];
            if (0..4).all(|a| (0..a).all(|b| t[a] != t[b])) {
                rank1680[code] = unrank1680.len() as u16;
                unrank1680.push(t);
            }
        }
        assert_eq!(unrank1680.len(), 1680);
        let unrank70: Vec<[u8; 4]> = unrank8
            .iter()
            .map(|&mask| {
                let mut t = [0u8; 4];
                let mut n = 0;
                for i in 0..8 {
                    if (mask >> i) & 1 == 1 {
                        t[n] = i as u8;
                        n += 1;
                    }
                }
                t
            })
            .collect();
        let pq = load_or_gen(&dir.join("phase4-555-edges.bin"), N_Q, || {
            let dist: Vec<AtomicU8> = (0..N_Q).into_par_iter().map(|_| AtomicU8::new(FAR)).collect();
            let home = [4u8, 5, 6, 7];
            dist[q_index(&rank8, &rank1680, &home, &home, &home)].store(0, Relaxed);
            let mut depth = 0u8;
            loop {
                let n: u64 = (0..N_Q)
                    .into_par_iter()
                    .with_min_len(1 << 14)
                    .map(|i| {
                        if dist[i].load(Relaxed) != depth {
                            return 0;
                        }
                        let (m, l, h) = (unrank70[i / (1680 * 1680)], unrank1680[i / 1680 % 1680], unrank1680[i % 1680]);
                        let mut new = 0;
                        for pm in &p_perm {
                            let j = q_index(&rank8, &rank1680, &m.map(|x| pm[0][x as usize]), &l.map(|x| pm[1][x as usize]), &h.map(|x| pm[2][x as usize]));
                            if dist[j].load(Relaxed) == FAR && dist[j].compare_exchange(FAR, depth + 1, Relaxed, Relaxed).is_ok() {
                                new += 1;
                            }
                        }
                        new
                    })
                    .sum();
                if n == 0 {
                    break;
                }
                depth += 1;
                eprintln!("  5x5x5 phase P table: depth {:2}  {:>10} new", depth, n);
            }
            let mut v = std::mem::ManuallyDrop::new(dist);
            // AtomicU8 has the same layout as u8.
            unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
        });
        // Phase P centres: F/B faces and R/L faces, to a side pattern the last phase accepts.
        let face_moves = |shift: u32| -> Vec<[u16; 16]> {
            (0..4900)
                .map(|i| {
                    let mut row = [0u16; 16];
                    for (k, &m) in P_MOVES.iter().enumerate() {
                        let (a, b) = (bx.apply(m, unrank8[i / 70] << shift) >> shift, bt.apply(m, unrank8[i % 70] << shift) >> shift);
                        assert!(a < 256 && b < 256, "these moves keep each face pair's centres");
                        row[k] = rank8[a as usize] as u16 * 70 + rank8[b as usize] as u16;
                    }
                    row
                })
                .collect()
        };
        let fb_mv = face_moves(8);
        let rl_mv = face_moves(16);
        let p_centres = load_or_gen(&dir.join("phase4-555-centres.bin"), 4900 * 4900, || {
            let mut dist = vec![FAR; 4900 * 4900];
            let mut frontier = Vec::new();
            for &id in &last_ids {
                let (x, t) = sides[id];
                let fb = rank8[(x >> 8 & 255) as usize] as usize * 70 + rank8[(t >> 8 & 255) as usize] as usize;
                let rl = rank8[(x >> 16) as usize] as usize * 70 + rank8[(t >> 16) as usize] as usize;
                dist[fb * 4900 + rl] = 0;
                frontier.push((fb * 4900 + rl) as u32);
            }
            let mut depth = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &x in &frontier {
                    let (fb, rl) = (x as usize / 4900, x as usize % 4900);
                    for k in 0..16 {
                        let y = fb_mv[fb][k] as usize * 4900 + rl_mv[rl][k] as usize;
                        if dist[y] == FAR {
                            dist[y] = depth + 1;
                            next.push(y as u32);
                        }
                    }
                }
                frontier = next;
                depth += 1;
            }
            dist
        });
        // Phase P, edges against centres.
        let wing_table = |kind: usize| -> Vec<[u32; 16]> {
            (0..N_MW)
                .into_par_iter()
                .map(|i| {
                    let (m, w) = (unrank70[i / 1680], unrank1680[i % 1680]);
                    let mut row = [0u32; 16];
                    for (k, pm) in p_perm.iter().enumerate() {
                        let w = w.map(|x| pm[kind][x as usize]);
                        row[k] = q_parts(&rank8, &rank1680, &m.map(|x| pm[0][x as usize]), &w, &w).0 as u32;
                    }
                    row
                })
                .collect()
        };
        let ml_mv = wing_table(1);
        let mh_mv = wing_table(2);
        let home = [4u8, 5, 6, 7];
        let mw_home = q_parts(&rank8, &rank1680, &home, &home, &home).0;
        // R/L patterns that occur, numbered.
        let mut rl_id = vec![u16::MAX; 4900];
        let mut rl_list = Vec::new();
        for rl in 0..4900 {
            if (0..4900).any(|fb| p_centres[fb * 4900 + rl] != FAR) {
                rl_id[rl] = rl_list.len() as u16;
                rl_list.push(rl);
            }
        }
        let n_rl = rl_list.len();
        let fb_goals: Vec<usize> = (0..4900).filter(|&fb| (0..4900).any(|rl| p_centres[fb * 4900 + rl] == 0)).collect();
        let rl_goals: Vec<usize> = (0..4900).filter(|&rl| (0..4900).any(|fb| p_centres[fb * 4900 + rl] == 0)).collect();
        let p_table = |name: &str, mv: &Vec<[u32; 16]>, fb: bool| {
            let file = dir.join(format!("phase4-555-{}.bin", name));
            if fb {
                let goals: Vec<(usize, usize)> = fb_goals.iter().map(|&c| (mw_home, c)).collect();
                load_or_gen(&file, (N_MW * 4900).div_ceil(2), || {
                    gen_pair(name, (N_MW, 4900, 16), |a, k| mv[a][k] as usize, |b, k| fb_mv[b][k] as usize, &goals)
                })
            } else {
                let goals: Vec<(usize, usize)> = rl_goals.iter().map(|&c| (mw_home, rl_id[c] as usize)).collect();
                load_or_gen(&file, (N_MW * n_rl).div_ceil(2), || {
                    gen_pair(
                        name,
                        (N_MW, n_rl, 16),
                        |a, k| mv[a][k] as usize,
                        |b, k| rl_id[rl_mv[rl_list[b]][k] as usize] as usize,
                        &goals,
                    )
                })
            }
        };
        let p_fb = [p_table("low-fb", &ml_mv, true), p_table("high-fb", &mh_mv, true)];
        let p_rl = [p_table("low-rl", &ml_mv, false), p_table("high-rl", &mh_mv, false)];
        // Phase E, edges against centres.
        let e_table = |name: &str, mv: &Vec<[u16; 14]>| {
            load_or_gen(&dir.join(format!("phase5-555-{}.bin", name)), (N_P8 * 4900).div_ceil(2), || {
                gen_pair(name, (N_P8, 4900, 14), |a, k| mv[a][k] as usize, |b, k| ud_mv[b][k] as usize, &[(0, ud_solved)])
            })
        };
        let e_ud = [e_table("low-ud", &out_low_mv), e_table("high-ud", &out_high_mv)];
        let e_side_table = |mv: &Vec<[u16; 14]>| -> Vec<u8> {
            let mut dist = vec![FAR; N_P8 * n_last];
            dist[0] = 0;
            let mut frontier = vec![0usize];
            let mut depth = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &x in &frontier {
                    let (a, b) = (x / n_last, x % n_last);
                    for k in 0..14 {
                        let y = mv[a][k] as usize * n_last + last_mv[b][k] as usize;
                        if dist[y] == FAR {
                            dist[y] = depth + 1;
                            next.push(y);
                        }
                    }
                }
                frontier = next;
                depth += 1;
            }
            dist
        };
        let e_side = [e_side_table(&out_low_mv), e_side_table(&out_high_mv)];

        Edges5 {
            t3, bw, bx, bt, rk, rank8, c, c_cls, c_phi, c_centres, out_low_mv, out_high_mv, e, ud_mv, side_id,
            side_last, last_mv, e_centres, outer_index, rank1680, ml_mv, mh_mv, pq, fb_mv, rl_mv, p_centres,
            p_fb, p_rl, rl_id, n_rl, e_ud, e_side,
        }
    }

    /// Phase-C distance for a wing mask and a flip coordinate.
    #[inline(always)]
    pub fn c_dist(&self, mask: u32, flip: usize) -> u8 {
        let cs = self.c_cls[self.rk.rank(mask)];
        nibble(&self.c, (cs >> 6) as usize * N_FLIP + self.c_phi[(cs & 63) as usize][flip] as usize)
    }

    #[inline(always)]
    fn e_mod3(&self, low: usize, high: usize) -> u8 {
        let idx = low * N_P8 + high;
        (self.e[idx >> 2] >> ((idx & 3) * 2)) & 3
    }

    /// Exact pairing distance of the outer edges, by walking down; `None` if unreachable.
    fn e_exact(&self, mut low: usize, mut high: usize) -> Option<u8> {
        let mut cur = self.e_mod3(low, high);
        if cur == 3 {
            return None;
        }
        let mut d = 0;
        while (low, high) != (0, 0) {
            let want = (cur + 2) % 3;
            let k = (0..14).find(|&k| {
                self.e_mod3(self.out_low_mv[low][k] as usize, self.out_high_mv[high][k] as usize) == want
            })?;
            low = self.out_low_mv[low][k] as usize;
            high = self.out_high_mv[high][k] as usize;
            cur = want;
            d += 1;
        }
        Some(d)
    }
}

/// A cube part of the way through its solution. Every phase searches from
/// many of these at once and returns the continuations that are shortest
/// in total, whichever start they belong to.
#[derive(Clone, Copy)]
pub struct Start {
    pub cube: Cube5,
    /// Moves made so far.
    pub cost: usize,
    /// The last of them, 255 if none.
    pub last: u8,
}

/// A phase solution: the start it continues, and its moves.
pub struct Found {
    pub start: usize,
    pub moves: Vec<u8>,
}

/// How far one phase search goes.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Stop at the first total length at which this many solutions exist.
    pub keep: usize,
    /// Never collect more than this many.
    pub cap: usize,
    /// Solutions kept per start and length.
    pub per_node: usize,
    /// Past this moment, stop as soon as there are `floor` solutions.
    pub deadline: Option<Instant>,
    pub floor: usize,
}

/// State shared by the searches of one phase.
struct Run {
    limits: Limits,
    count: AtomicUsize,
    stop: AtomicBool,
}

/// One start of a phase search: lower bound on the total, moves so far, payload.
struct Node<N> {
    est: usize,
    cost: usize,
    start: usize,
    last: u8,
    data: N,
}

struct Search<'a> {
    e5: &'a Edges5,
    run: &'a Run,
    path: [u8; 40],
    out: Vec<Vec<u8>>,
    ticks: u32,
}

impl Search<'_> {
    /// Records the path as a solution; true if this search should end.
    fn found(&mut self, depth: usize) -> bool {
        self.out.push(self.path[..depth].to_vec());
        let n = self.run.count.fetch_add(1, Relaxed) + 1;
        if n >= self.run.limits.cap {
            self.run.stop.store(true, Relaxed);
        }
        self.out.len() >= self.run.limits.per_node || n >= self.run.limits.cap
    }

    /// True if the phase has been told to stop.
    #[inline(always)]
    fn tick(&mut self) -> bool {
        self.ticks += 1;
        if self.ticks & 1023 != 0 {
            return false;
        }
        if self.run.stop.load(Relaxed) {
            return true;
        }
        if self.run.count.load(Relaxed) >= self.run.limits.floor
            && self.run.limits.deadline.is_some_and(|d| Instant::now() > d)
        {
            self.run.stop.store(true, Relaxed);
            return true;
        }
        false
    }
}

/// True if every edge has its middle piece and both wings, the right way round,
/// and every centre is home.
pub fn is_reduced(c: &Cube5) -> bool {
    primary(&c.c4.ct) == SOLVED_CENTRES
        && primary(&c.t) == SOLVED_CENTRES
        && (0..12).all(|s| {
            let (lo, hi) = (c.c4.ep[s], c.c4.ep[s + 12]);
            lo % 12 == c.mid.ep[s] && hi % 12 == c.mid.ep[s] && lo / 12 == c.mid.eo[s] && hi / 12 != c.mid.eo[s]
        })
}

/// Per class: the slot of its middle edge, of its low wing and of its high wing.
fn slots_of(cube: &Cube5) -> ([u8; 12], [u8; 12], [u8; 12]) {
    let (mut mid, mut low, mut high) = ([0u8; 12], [0u8; 12], [0u8; 12]);
    for s in 0..12 {
        mid[cube.mid.ep[s] as usize] = s as u8;
        low[(cube.c4.ep[s] % 12) as usize] = s as u8;
        high[(cube.c4.ep[s + 12] % 12) as usize] = s as u8;
    }
    (mid, low, high)
}

/// Phase C state: where the low-labelled wings are, the flips of the middle
/// edges against the labelling, and the centre patterns.
#[derive(Clone, Copy)]
struct CNode {
    mask: u32,
    flip: usize,
    x: u32,
    t: u32,
}

/// Phase P state: positions of the four classes' pieces among the outer
/// edges, the centre patterns of the F/B and R/L faces, and the cube itself.
#[derive(Clone, Copy)]
struct PNode {
    ml: usize,
    mh: usize,
    fb: usize,
    rl: usize,
}

/// Phase E state: pairing permutations of the outer edges, their exact
/// distance, and the centre patterns.
#[derive(Clone, Copy)]
struct ENode {
    lo: usize,
    hi: usize,
    d: u8,
    ud: usize,
    side: usize,
}

impl Edges5 {
    fn rl_pattern(colours: &[u8; 24]) -> u32 {
        (16..24).fold(0, |m, i| m | ((colours[i] == 1) as u32) << i)
    }

    /// Searches every node to the same total length, shortest total first,
    /// until enough solutions exist.
    fn by_total<N: Sync>(
        &self,
        mut nodes: Vec<Node<N>>,
        limits: &Limits,
        search: impl Fn(&mut Search, &Node<N>, usize) + Sync,
    ) -> Vec<Found> {
        nodes.sort_by_key(|n| (n.est, n.cost));
        let Some(first) = nodes.first().map(|n| n.est) else { return Vec::new() };
        let run = Run { limits: *limits, count: AtomicUsize::new(0), stop: AtomicBool::new(false) };
        let mut out = Vec::new();
        for total in first..=first + 12 {
            let live = nodes.partition_point(|n| n.est <= total);
            let found: Vec<Found> = nodes[..live]
                .par_iter()
                .with_max_len(1)
                .flat_map_iter(|n| {
                    let mut s = Search { e5: self, run: &run, path: [0; 40], out: Vec::new(), ticks: 0 };
                    if !run.stop.load(Relaxed) {
                        search(&mut s, n, total - n.cost);
                    }
                    s.out.into_iter().map(move |moves| Found { start: n.start, moves })
                })
                .collect();
            if std::env::var_os("CUBE5_GAPS").is_some() && !found.is_empty() {
                let mut gaps = [0usize; 8];
                for f in &found {
                    let est = nodes[..live].iter().filter(|n| n.start == f.start).map(|n| n.est).min().unwrap();
                    gaps[(total - est).min(7)] += 1;
                }
                eprintln!("    total {} (first {}): {} live nodes, solutions by gap {:?}", total, first, live, gaps);
            }
            out.extend(found);
            if out.len() >= limits.keep || run.stop.load(Relaxed) {
                break;
            }
            if out.len() >= limits.floor.max(1) && limits.deadline.is_some_and(|d| Instant::now() > d) {
                break;
            }
        }
        out
    }

    /// The best labellings for phase C: (lower bound, labelling), best first.
    fn c_labellings(&self, cube: &Cube5, keep: usize) -> Vec<(u8, u16)> {
        let mut wings = [0u32; 12];
        let mut mids = [0u16; 12];
        let (mut mask, mut eo) = (0u32, 0u16);
        for p in 0..24 {
            let piece = cube.c4.ep[p];
            wings[(piece % 12) as usize] |= 1 << p;
            if piece < 12 {
                mask |= 1 << p;
            }
        }
        for s in 0..12 {
            mids[cube.mid.ep[s] as usize] = 1 << s;
            eo |= (cube.mid.eo[s] as u16) << s;
        }
        let centres = self.c_centres[self.rank8[(Self::rl_pattern(&cube.c4.ct) >> 16) as usize] as usize * 70
            + self.rank8[(Self::rl_pattern(&cube.t) >> 16) as usize] as usize];
        let mut all = Vec::with_capacity(2048);
        let mut lab = 0u16;
        for i in 0..4096u32 {
            if i > 0 {
                // Gray code: one class changes its labelling per step.
                let c = i.trailing_zeros() as usize;
                lab ^= 1 << c;
                mask ^= wings[c];
                eo ^= mids[c];
            }
            if lab.count_ones() % 2 == 0 {
                all.push((self.c_dist(mask, (eo & 0x7FF) as usize).max(centres), lab));
            }
        }
        all.sort();
        all.truncate(keep);
        all
    }

    fn c_node(&self, cube: &Cube5, lab: u16) -> CNode {
        let (mut mask, mut eo) = (0u32, 0usize);
        for p in 0..24 {
            let piece = cube.c4.ep[p];
            if (piece / 12) as u16 ^ ((lab >> (piece % 12)) & 1) == 0 {
                mask |= 1 << p;
            }
        }
        for s in 0..12 {
            eo |= ((cube.mid.eo[s] as u16 ^ ((lab >> cube.mid.ep[s]) & 1)) as usize) << s;
        }
        CNode { mask, flip: eo & 0x7FF, x: Self::rl_pattern(&cube.c4.ct), t: Self::rl_pattern(&cube.t) }
    }

    #[inline(always)]
    fn c_bound(&self, n: &CNode) -> usize {
        let centres =
            self.c_centres[self.rank8[(n.x >> 16) as usize] as usize * 70 + self.rank8[(n.t >> 16) as usize] as usize];
        self.c_dist(n.mask, n.flip).max(centres) as usize
    }

    /// Phase C from many starts, each with its best `labellings` labellings.
    pub fn phase_c(&self, starts: &[Start], labellings: usize, limits: &Limits) -> Vec<Found> {
        let nodes: Vec<Node<CNode>> = starts
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, s)| {
                self.c_labellings(&s.cube, labellings).into_iter().map(move |(bound, lab)| Node {
                    est: s.cost + bound as usize,
                    cost: s.cost,
                    start: i,
                    last: s.last,
                    data: self.c_node(&s.cube, lab),
                })
            })
            .collect();
        self.by_total(nodes, limits, |s, n, rem| {
            s.dfs_c(&n.data, rem, n.last, 0);
        })
    }

    fn p_node(&self, cube: &Cube5, classes: &[u8; 4], slots: &([u8; 12], [u8; 12], [u8; 12])) -> PNode {
        let at = |slots: &[u8; 12]| classes.map(|c| self.outer_index[slots[c as usize] as usize]);
        let (x, t) = (primary(&cube.c4.ct), primary(&cube.t));
        let id = |shift: u32| {
            self.rank8[(x >> shift & 255) as usize] as usize * 70 + self.rank8[(t >> shift & 255) as usize] as usize
        };
        let (ml, mh) = q_parts(&self.rank8, &self.rank1680, &at(&slots.0), &at(&slots.1), &at(&slots.2));
        PNode { ml, mh, fb: id(8), rl: id(16) }
    }

    /// Lower bound for phase P, or `over` as soon as one table exceeds `max`.
    #[inline(always)]
    fn p_bound(&self, n: &PNode, max: usize) -> usize {
        let mut b = self.pq[n.ml * 1680 + n.mh % 1680].max(self.p_centres[n.fb * 4900 + n.rl]) as usize;
        if b > max {
            return b;
        }
        b = b.max(nibble(&self.p_fb[0], n.ml * 4900 + n.fb) as usize);
        if b > max {
            return b;
        }
        b = b.max(nibble(&self.p_fb[1], n.mh * 4900 + n.fb) as usize);
        if b > max {
            return b;
        }
        let rl = self.rl_id[n.rl] as usize;
        if rl >= self.n_rl {
            return FAR as usize;
        }
        b.max(nibble(&self.p_rl[0], n.ml * self.n_rl + rl) as usize).max(nibble(&self.p_rl[1], n.mh * self.n_rl + rl) as usize)
    }

    /// Phase P from many starts: every choice of four classes with no piece
    /// in the slice between F and B is a node of its own.
    pub fn phase_p(&self, starts: &[Start], limits: &Limits) -> Vec<Found> {
        let nodes: Vec<Node<(PNode, Cube5)>> = starts
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, s)| {
                let slots = slots_of(&s.cube);
                let free: Vec<u8> = (0..12u8)
                    .filter(|&c| {
                        [slots.0, slots.1, slots.2].iter().all(|k| self.outer_index[k[c as usize] as usize] != 255)
                    })
                    .collect();
                let mut out = Vec::new();
                for a in 0..free.len() {
                    for b in a + 1..free.len() {
                        for c in b + 1..free.len() {
                            for d in c + 1..free.len() {
                                let node = self.p_node(&s.cube, &[free[a], free[b], free[c], free[d]], &slots);
                                let bound = self.p_bound(&node, usize::MAX);
                                if bound != FAR as usize {
                                    out.push(Node {
                                        est: s.cost + bound,
                                        cost: s.cost,
                                        start: i,
                                        last: s.last,
                                        data: (node, s.cube),
                                    });
                                }
                            }
                        }
                    }
                }
                out
            })
            .collect();
        self.by_total(nodes, limits, |s, n, rem| {
            s.dfs_p(&n.data.1, &n.data.0, rem, n.last, 0);
        })
    }

    /// Pairing permutations of the outer edges, if the middle slice holds four whole classes.
    fn outer_perms(&self, cube: &Cube5) -> Option<(usize, usize)> {
        let (mid, _, _) = slots_of(cube);
        let (mut lo, mut hi) = ([0u8; 8], [0u8; 8]);
        for s in 0..8 {
            lo[s] = mid[(cube.c4.ep[s] % 12) as usize];
            hi[s] = mid[(cube.c4.ep[s + 12] % 12) as usize];
            if lo[s] >= 8 || hi[s] >= 8 {
                return None;
            }
        }
        Some((perm_rank(&lo), perm_rank(&hi)))
    }

    /// Phase-E start of a cube whose middle slice is complete, or `None`.
    fn e_node(&self, cube: &Cube5) -> Option<ENode> {
        let (lo, hi) = self.outer_perms(cube)?;
        let d = self.e_exact(lo, hi)?;
        let (x, t) = (primary(&cube.c4.ct), primary(&cube.t));
        let ud = self.rank8[(x & 255) as usize] as usize * 70 + self.rank8[(t & 255) as usize] as usize;
        let side = *self.side_id.get(&((x >> 8) << 16 | (t >> 8)))? as usize;
        if self.side_last[side] == FAR {
            return None;
        }
        let n = ENode { lo, hi, d, ud, side: self.side_last[side] as usize };
        (self.e_centre(&n) != FAR).then_some(n)
    }

    #[inline(always)]
    fn e_centre(&self, n: &ENode) -> u8 {
        self.e_centres[n.ud * 36 + n.side]
    }

    /// What the edge-against-centre tables say about a phase-E state.
    #[inline(always)]
    fn e_joint(&self, n: &ENode) -> u8 {
        nibble(&self.e_ud[0], n.lo * 4900 + n.ud)
            .max(nibble(&self.e_ud[1], n.hi * 4900 + n.ud))
            .max(self.e_side[0][n.lo * 36 + n.side])
            .max(self.e_side[1][n.hi * 36 + n.side])
    }

    fn e_bound(&self, n: &ENode) -> usize {
        n.d.max(self.e_centre(n)).max(self.e_joint(n)) as usize
    }

    /// Phase E from many starts; the results are reduced cubes.
    pub fn phase_e(&self, starts: &[Start], limits: &Limits) -> Vec<Found> {
        let nodes: Vec<Node<ENode>> = starts
            .par_iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let node = self.e_node(&s.cube)?;
                Some(Node { est: s.cost + self.e_bound(&node), cost: s.cost, start: i, last: s.last, data: node })
            })
            .collect();
        self.by_total(nodes, limits, |s, n, rem| {
            s.dfs_e(&n.data, rem, n.last, 0);
        })
    }

    /// Development aid: scramble a solved cube inside each phase's own moves
    /// and check that the phase finds its way back.
    pub fn self_test(&self) {
        let mut seed = 99u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        let limits = Limits { keep: 1, cap: 1, per_node: 1, deadline: None, floor: 1 };
        for round in 0..4 {
            let mut c = Cube5::solved();
            for _ in 0..40 {
                c.apply(E_MOVES[rnd(14)]);
            }
            let t = Instant::now();
            let sols = self.phase_e(&[Start { cube: c, cost: 0, last: 255 }], &limits);
            eprintln!("E round {}: {:?} moves, {:.0} ms", round, sols.first().map(|s| s.moves.len()), t.elapsed().as_secs_f64() * 1e3);
            c.apply_seq(&sols[0].moves);
            assert!(is_reduced(&c));
        }
        for round in 0..4 {
            let mut c = Cube5::solved();
            for _ in 0..40 {
                c.apply(P_MOVES[rnd(16)]);
            }
            let t = Instant::now();
            let sols = self.phase_p(&[Start { cube: c, cost: 0, last: 255 }], &limits);
            eprintln!("P round {}: {:?} moves, {:.0} ms", round, sols.first().map(|s| s.moves.len()), t.elapsed().as_secs_f64() * 1e3);
            c.apply_seq(&sols[0].moves);
            assert!(self.e_node(&c).is_some());
        }
    }
}

impl Search<'_> {
    fn dfs_c(&mut self, n: &CNode, rem: usize, last: u8, depth: usize) -> bool {
        if rem == 0 {
            return n.mask == LOW_HALF && n.flip == 0 && self.e5.c_bound(n) == 0 && self.found(depth);
        }
        if self.tick() {
            return true;
        }
        let e = self.e5;
        for &m in &H_MOVES {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let next = CNode {
                mask: e.bw.apply(m, n.mask),
                flip: e.t3.flip_mv[n.flip][m as usize % 18] as usize,
                x: e.bx.apply(m, n.x),
                t: e.bt.apply(m, n.t),
            };
            let bound = e.c_bound(&next);
            // A cube that is ready early was a shorter solution already.
            if bound > rem - 1 || (bound == 0 && rem > 1) {
                continue;
            }
            self.path[depth] = m;
            if self.dfs_c(&next, rem - 1, m, depth + 1) {
                return true;
            }
        }
        false
    }

    fn dfs_p(&mut self, root: &Cube5, n: &PNode, rem: usize, last: u8, depth: usize) -> bool {
        let e = self.e5;
        if rem == 0 {
            // Four whole edges are in the middle slice; phase E must also accept the rest.
            let mut c = *root;
            c.apply_seq(&self.path[..depth]);
            return e.e_node(&c).is_some() && self.found(depth);
        }
        if self.tick() {
            return true;
        }
        for (k, &m) in P_MOVES.iter().enumerate() {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let next = PNode {
                ml: e.ml_mv[n.ml][k] as usize,
                mh: e.mh_mv[n.mh][k] as usize,
                fb: e.fb_mv[n.fb][k] as usize,
                rl: e.rl_mv[n.rl][k] as usize,
            };
            let bound = e.p_bound(&next, rem - 1);
            if bound > rem - 1 || (bound == 0 && rem > 1) {
                continue;
            }
            self.path[depth] = m;
            if self.dfs_p(root, &next, rem - 1, m, depth + 1) {
                return true;
            }
        }
        false
    }

    fn dfs_e(&mut self, n: &ENode, rem: usize, last: u8, depth: usize) -> bool {
        if rem == 0 {
            return n.d == 0 && self.e5.e_centre(n) == 0 && self.found(depth);
        }
        if self.tick() {
            return true;
        }
        let e = self.e5;
        for (k, &m) in E_MOVES[..14].iter().enumerate() {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let (lo, hi) = (e.out_low_mv[n.lo][k] as usize, e.out_high_mv[n.hi][k] as usize);
            // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
            let v = e.e_mod3(lo, hi);
            let d = n.d + (v + 3 - (n.d + 2) % 3) % 3 - 1;
            if d as usize > rem - 1 {
                continue;
            }
            let next = ENode { lo, hi, d, ud: e.ud_mv[n.ud][k] as usize, side: e.last_mv[n.side][k] as usize };
            let centre = e.e_centre(&next) as usize;
            if centre > rem - 1 || (d == 0 && centre == 0 && rem > 1) || e.e_joint(&next) as usize > rem - 1 {
                continue;
            }
            self.path[depth] = m;
            if self.dfs_e(&next, rem - 1, m, depth + 1) {
                return true;
            }
        }
        false
    }
}

/// The same phases for a 5x5x5 that is the inside of a larger cube.
pub mod rider;

/// Phase E pairing table: distance modulo 3 for every pair of pairing
/// permutations (low wings, high wings) of the eight outer edges; 3 = unreachable.
fn gen_e(low_mv: &[[u16; 14]], high_mv: &[[u16; 14]]) -> Vec<u8> {
    let bytes: Vec<AtomicU8> = (0..N_E.div_ceil(4)).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    let get = |idx: usize| (bytes[idx >> 2].load(Relaxed) >> ((idx & 3) * 2)) & 3;
    let set = |idx: usize, v: u8| -> bool {
        let shift = (idx & 3) * 2;
        let cell = &bytes[idx >> 2];
        let mut cur = cell.load(Relaxed);
        loop {
            if (cur >> shift) & 3 != 3 {
                return false;
            }
            match cell.compare_exchange_weak(cur, cur & !((3 ^ v) << shift), Relaxed, Relaxed) {
                Ok(_) => return true,
                Err(now) => cur = now,
            }
        }
    };
    set(0, 0);
    let mut done = 1u64;
    let mut depth = 0usize;
    let mut frontier: Vec<u64> = vec![0];
    while !frontier.is_empty() {
        let v = ((depth + 1) % 3) as u8;
        let keep = frontier.len() <= 3_000_000;
        let next: Vec<(Vec<u64>, u64)> = frontier
            .par_chunks(4096)
            .map(|chunk| {
                let (mut out, mut n) = (Vec::new(), 0u64);
                for &x in chunk {
                    let (a, b) = (x as usize / N_P8, x as usize % N_P8);
                    for k in 0..14 {
                        let y = low_mv[a][k] as usize * N_P8 + high_mv[b][k] as usize;
                        if set(y, v) {
                            n += 1;
                            if keep {
                                out.push(y as u64);
                            }
                        }
                    }
                }
                (out, n)
            })
            .collect();
        let n: u64 = next.iter().map(|p| p.1).sum();
        frontier = next.into_iter().flat_map(|p| p.0).collect();
        depth += 1;
        done += n;
        eprintln!("  5x5x5 phase E table: depth {:2}  {:>10} new  {:>10} total", depth, n, done);
        if !keep {
            break;
        }
    }
    drop(frontier);
    loop {
        let target = (depth % 3) as u8;
        let v = ((depth + 1) % 3) as u8;
        let n: u64 = (0..N_P8)
            .into_par_iter()
            .map(|a| {
                let base = a * N_P8;
                let mut unseen: Vec<u16> = (0..N_P8).filter(|&b| get(base + b) == 3).map(|b| b as u16).collect();
                let mut found = 0u64;
                for k in 0..14 {
                    if unseen.is_empty() {
                        break;
                    }
                    let nb = low_mv[a][k] as usize * N_P8;
                    let mut i = 0;
                    while i < unseen.len() {
                        let b = unseen[i] as usize;
                        if get(nb + high_mv[b][k] as usize) == target {
                            set(base + b, v);
                            found += 1;
                            unseen.swap_remove(i);
                        } else {
                            i += 1;
                        }
                    }
                }
                found
            })
            .sum();
        if n == 0 {
            break;
        }
        depth += 1;
        done += n;
        eprintln!("  5x5x5 phase E table: depth {:2}  {:>10} new  {:>10} total", depth, n, done);
    }
    eprintln!("  5x5x5 phase E table: {} of {} states reachable", done, N_E);
    let mut v = std::mem::ManuallyDrop::new(bytes);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

