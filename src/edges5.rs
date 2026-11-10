//! 5x5x5, second part: from staged centres to a reduced cube.
//!
//! Wing positions `s` and `s + 12` and middle-edge slot `s` form edge `s`.
//! A wing piece `j` has class `j % 12` (its two colours) and type `j / 12`;
//! positions 0..12 are the "low" half, 12..24 the "high" half.
//!
//! Phase C (face turns and wide half turns) makes the edges ready to be
//! paired by moves that never mix the halves: every class gets one wing in
//! each half, and its low wing agrees with the flip of its middle edge.
//! Phase D completes four edges in the middle slice, phase E the other eight.

use crate::c3::{perm_rank, perm_unrank, T3};
use crate::c5::{tables5, Cube5};
use crate::cube::{skip_move, tables as cube_tables};
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

/// Face turns and wide half turns: everything that keeps the centres staged.
pub const H_MOVES: [u8; 24] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 22, 25, 28, 31, 34];

const N_MASK: usize = 2_704_156; // 12 of 24
const N_FLIP: usize = 2048;
const N_C: usize = N_MASK * N_FLIP;
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

/// Phase C: moves until the low-labelled wings fill the low half and the
/// (labelled) middle edges are unflipped, for every (wing mask, flip) pair.
/// Four bits per entry, 2.77 GB; exact up to 14 moves.
fn gen_c(bw: &BitPerm, flip_mv: &[[u16; 18]], rk: &MaskRank) -> Vec<u8> {
    let bytes: Vec<AtomicU8> = {
        let mut v = vec![0xFFu8; N_C / 2];
        let p = v.as_mut_ptr() as *mut AtomicU8;
        let (len, cap) = (v.len(), v.capacity());
        std::mem::forget(v);
        // AtomicU8 has the same layout as u8.
        unsafe { Vec::from_raw_parts(p, len, cap) }
    };
    let goal = rk.rank(LOW_HALF) * N_FLIP;
    nib_set(&bytes, goal, 0);
    let mut done = 1u64;
    let mut depth = 0u8;
    let mut frontier: Vec<u64> = vec![goal as u64];
    // Push from explicit frontiers while they are small.
    while !frontier.is_empty() {
        let keep = frontier.len() <= 3_000_000;
        let next: Vec<(Vec<u64>, u64)> = frontier
            .par_chunks(4096)
            .map(|chunk| {
                let (mut out, mut n) = (Vec::new(), 0u64);
                for &x in chunk {
                    let (mask, flip) = (rk.unrank[x as usize / N_FLIP], x as usize % N_FLIP);
                    for &m in &H_MOVES {
                        let y = rk.rank(bw.apply(m, mask)) * N_FLIP + flip_mv[flip][m as usize % 18] as usize;
                        if nib_set(&bytes, y, depth + 1) {
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
        eprintln!("  5x5x5 phase C table: depth {:2}  {:>10} new  {:>10} total", depth, n, done);
        if !keep {
            break;
        }
    }
    drop(frontier);
    // Then pull: every unseen state looks for a neighbour at the current depth.
    // A couple of thousand states need 15 moves; they are stored as 14, which
    // keeps the table a lower bound.
    while done < N_C as u64 && depth < UNSEEN - 1 {
        let n: u64 = (0..N_MASK)
            .into_par_iter()
            .with_min_len(64)
            .map(|b| {
                let base = b * N_FLIP;
                let mut unseen: Vec<u16> = Vec::new();
                for f in 0..N_FLIP {
                    if nib(&bytes, base + f) == UNSEEN {
                        unseen.push(f as u16);
                    }
                }
                if unseen.is_empty() {
                    return 0;
                }
                let mask = rk.unrank[b];
                let mut found = 0u64;
                for &m in &H_MOVES {
                    let nb = rk.rank(bw.apply(m, mask)) * N_FLIP;
                    let mut i = 0;
                    while i < unseen.len() {
                        let f = unseen[i] as usize;
                        if nib(&bytes, nb + flip_mv[f][m as usize % 18] as usize) == depth {
                            nib_set(&bytes, base + f, depth + 1);
                            found += 1;
                            unseen.swap_remove(i);
                        } else {
                            i += 1;
                        }
                    }
                    if unseen.is_empty() {
                        break;
                    }
                }
                found
            })
            .sum();
        assert!(n > 0, "phase C table: unreachable states remain");
        depth += 1;
        done += n;
        eprintln!("  5x5x5 phase C table: depth {:2}  {:>10} new  {:>10} total", depth, n, done);
    }
    eprintln!("  5x5x5 phase C table: {} states further than {} moves", N_C as u64 - done, depth);
    bytes.par_iter().for_each(|cell| {
        let mut x = cell.load(Relaxed);
        if x & 15 == UNSEEN {
            x -= 1;
        }
        if x >> 4 == UNSEEN {
            x -= 16;
        }
        cell.store(x, Relaxed);
    });
    let mut v = std::mem::ManuallyDrop::new(bytes);
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

/// Moves that never mix the wing halves and never flip a middle edge:
/// U D F B freely, R2 L2, and the six wide half turns.
pub const G_MOVES: [u8; 20] = [0, 1, 2, 6, 7, 8, 9, 10, 11, 15, 16, 17, 4, 13, 19, 22, 25, 28, 31, 34];
/// Moves that keep the pieces of the middle slice in the middle slice:
/// U D freely, the other face half turns, and the six wide half turns.
/// The first 14 never touch the pairing of the middle-slice edges.
pub const E_MOVES: [u8; 16] = [0, 1, 2, 9, 10, 11, 4, 7, 13, 16, 22, 25, 31, 34, 19, 28];

const N_SET: usize = 495; // 4 slots out of 12
const N_D: usize = N_SET * N_SET * N_SET;
const N_P8: usize = 40320;
const N_E: usize = N_P8 * N_P8;
const N_IN: usize = 24 * 24;
const MIDDLE: usize = 0xF00;
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
    c: Bytes,
    /// Phase C centres: moves until the R-vs-L patterns on the R and L faces
    /// are ones the later phases can finish, by (x pattern, t pattern).
    c_centres: Vec<u8>,
    /// Phase D: moves until the middle slice holds the middle edges, low wings
    /// and high wings of the same four classes, by the three sets of slots.
    set_mv: [Vec<[u16; 20]>; 3],
    d: Bytes,
    /// Phase D centres: moves until each orbit is in a pattern phase E accepts.
    d_x: Vec<u8>,
    d_t: Vec<u8>,
    /// Phase E, outer edges: pairing permutations, their moves, and the exact
    /// pairing table (modulo 3).
    out_low_mv: Vec<[u16; 14]>,
    out_high_mv: Vec<[u16; 14]>,
    e: Bytes,
    /// Phase E, middle slice and centres: pairing permutations of the four
    /// middle edges with their moves, centre patterns with theirs, and the
    /// exact table of the two together.
    in_low_mv: Vec<[u8; 16]>,
    in_high_mv: Vec<[u8; 16]>,
    /// Centres of the U and D faces (x pattern, t pattern) and their moves.
    ud_mv: Vec<[u16; 16]>,
    /// Centres of the four side faces: the patterns phase E can reach, numbered.
    side_id: std::collections::HashMap<u32, u32>,
    side_mv: Vec<[u32; 16]>,
    /// Phase D2: exact moves until the middle slice is paired and the side
    /// centres are in one of the patterns the last phase can finish.
    d2: Bytes,
    /// Those side patterns, numbered, with their moves under the last phase.
    side_last: Vec<u8>,
    last_mv: Vec<[u8; 14]>,
    /// Phase E centres: exact moves to solve (U/D pattern, side pattern).
    e_centres: Vec<u8>,
}

/// A cube ready for phase E.
pub struct Candidate {
    /// Moves of phases C and D.
    pub moves: Vec<u8>,
    pub len_c: usize,
    pub cube: Cube5,
    /// Lower bound on the length of phases C, D and E together.
    pub bound: usize,
}

/// A reduction found by the edge phases.
pub struct Reduction {
    /// Moves of phases C, D and E.
    pub moves: Vec<u8>,
    pub lens: [usize; 3],
    pub cube: Cube5,
}

fn centre_index(rank8: &[u8; 256], mask: u32) -> usize {
    let (a, b, c) = (
        rank8[(mask & 255) as usize] as usize,
        rank8[((mask >> 8) & 255) as usize] as usize,
        rank8[(mask >> 16) as usize] as usize,
    );
    (a * 70 + b) * 70 + c
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
        let c = load_or_gen(&dir.join("phase3-555.bin"), N_C / 2, || gen_c(&bw, &t3.flip_mv, &rk));
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
        let g_slots: Vec<SlotMove> = G_MOVES.iter().map(|&m| slot_move(m)).collect();
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
        // Phase D centres: per orbit, G moves until the side faces show a pattern phase E accepts.
        let orbit_table = |bp: &BitPerm, pick: &dyn Fn(&(u32, u32)) -> u32| -> Vec<u8> {
            let accepted: std::collections::HashSet<u32> = sides.iter().map(|st| pick(st) >> 8).collect();
            let mut dist = vec![FAR; 343_000];
            let mut frontier = Vec::new();
            for &side in &accepted {
                for &ud in &unrank8 {
                    let m = side << 8 | ud;
                    dist[centre_index(&rank8, m)] = 0;
                    frontier.push(m);
                }
            }
            let mut depth = 0;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &x in &frontier {
                    for &m in &G_MOVES {
                        let y = bp.apply(m, x);
                        if dist[centre_index(&rank8, y)] == FAR {
                            dist[centre_index(&rank8, y)] = depth + 1;
                            next.push(y);
                        }
                    }
                }
                frontier = next;
                depth += 1;
            }
            dist
        };
        let d_x = orbit_table(&bx, &|st| st.0);
        let d_t = orbit_table(&bt, &|st| st.1);

        // Phase D: sets of four slots.
        let set_table = |kind: usize| -> Vec<[u16; 20]> {
            (0..N_SET)
                .map(|r| {
                    let mask = t3.slice_unrank[r];
                    let mut row = [0u16; 20];
                    for (k, sm) in g_slots.iter().enumerate() {
                        let p = [&sm.mid, &sm.low, &sm.high][kind];
                        let out = (0..12).filter(|s| (mask >> s) & 1 == 1).fold(0usize, |a, s| a | 1 << p[s]);
                        row[k] = t3.slice_rank[out];
                    }
                    row
                })
                .collect()
        };
        let set_mv = [set_table(0), set_table(1), set_table(2)];
        let d = load_or_gen(&dir.join("phase4-555.bin"), N_D, || {
            let goal = t3.slice_rank[MIDDLE] as usize;
            let mut dist = vec![FAR; N_D];
            let start = (goal * N_SET + goal) * N_SET + goal;
            dist[start] = 0;
            let mut frontier = vec![start as u32];
            let mut depth = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &x in &frontier {
                    let (a, b, c) = (x as usize / (N_SET * N_SET), x as usize / N_SET % N_SET, x as usize % N_SET);
                    for k in 0..20 {
                        let y = (set_mv[0][a][k] as usize * N_SET + set_mv[1][b][k] as usize) * N_SET
                            + set_mv[2][c][k] as usize;
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
        // Phase E, middle slice.
        let inner_table = |high: bool| -> Vec<[u8; 16]> {
            (0..24)
                .map(|r| {
                    let mut sigma = [0u8; 4];
                    perm_unrank(r, &mut sigma);
                    let mut row = [0u8; 16];
                    for (k, sm) in e_slots.iter().enumerate() {
                        let wing = if high { &sm.high } else { &sm.low };
                        let mut out = [0u8; 4];
                        for s in 0..4 {
                            out[wing[s + 8] as usize - 8] = sm.mid[sigma[s] as usize + 8] - 8;
                        }
                        row[k] = perm_rank(&out) as u8;
                    }
                    row
                })
                .collect()
        };
        let in_low_mv = inner_table(false);
        let in_high_mv = inner_table(true);
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
        // Phase D2: middle-slice pairing with the side centres, to any of those patterns.
        let d2 = load_or_gen(&dir.join("phase4b-555.bin"), N_IN * n_side, || {
            let mut dist = vec![FAR; N_IN * n_side];
            let mut frontier: Vec<u32> = last_ids.iter().map(|&id| id as u32).collect();
            for &x in &frontier {
                dist[x as usize] = 0;
            }
            let mut depth = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &x in &frontier {
                    let (pair, c) = (x as usize / n_side, x as usize % n_side);
                    for k in 0..16 {
                        let y = (in_low_mv[pair / 24][k] as usize * 24 + in_high_mv[pair % 24][k] as usize) * n_side
                            + side_mv[c][k] as usize;
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

        Edges5 {
            t3, bw, bx, bt, rk, rank8, c, c_centres, set_mv, d, d_x, d_t, out_low_mv, out_high_mv, e,
            in_low_mv, in_high_mv, ud_mv, side_id, side_mv, d2, side_last, last_mv, e_centres,
        }
    }

    /// Phase-C distance for a wing mask and a flip coordinate.
    #[inline(always)]
    pub fn c_dist(&self, mask: u32, flip: usize) -> u8 {
        let idx = self.rk.rank(mask) * N_FLIP + flip;
        (self.c[idx >> 1] >> ((idx & 1) * 4)) & 15
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

/// How wide the edge search goes.
#[derive(Clone, Debug)]
pub struct EdgeParams {
    /// Labellings (final flip patterns) tried in phase C.
    pub labellings: usize,
    /// Phase-C solutions kept per labelling.
    pub c_keep: usize,
    /// Choices of the four middle-slice classes tried per phase-C solution.
    pub subsets: usize,
    /// Phase-D solutions kept per choice.
    pub d_keep: usize,
    /// How many moves past the shortest phase-D solution to keep collecting.
    pub d_extra: usize,
    /// Ways of pairing the middle slice kept per phase-D solution.
    pub d2_keep: usize,
    /// Candidates completed by phase E.
    pub e_starts: usize,
}

impl Default for EdgeParams {
    fn default() -> EdgeParams {
        EdgeParams { labellings: 2, c_keep: 10, subsets: 6, d_keep: 20, d_extra: 1, d2_keep: 3, e_starts: 60 }
    }
}

struct Search<'a> {
    e5: &'a Edges5,
    path: [u8; 40],
    out: Vec<Vec<u8>>,
    limit: usize,
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

/// Start of phase E: pairing permutations of the outer and middle edges,
/// the exact outer distance, and the centre pattern.
#[derive(Clone, Copy)]
struct ENode {
    lo: usize,
    hi: usize,
    d: u8,
    ud: usize,
    side: usize,
}

/// Phase D2 state: pairing permutations of the middle slice and the side centres.
#[derive(Clone, Copy)]
struct D2Node {
    ilo: usize,
    ihi: usize,
    side: usize,
}

#[derive(Clone, Copy)]
struct DNode {
    m: usize,
    l: usize,
    h: usize,
    x: u32,
    t: u32,
}

impl Edges5 {
    fn rl_pattern(colours: &[u8; 24]) -> u32 {
        (16..24).fold(0, |m, i| m | ((colours[i] == 1) as u32) << i)
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

    /// Phase C for one labelling: all solutions of the shortest length (up to `limit`).
    fn solve_c(&self, cube: &Cube5, lab: u16, last: u8, limit: usize) -> Vec<Vec<u8>> {
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
        let (x, t) = (Self::rl_pattern(&cube.c4.ct), Self::rl_pattern(&cube.t));
        let mut s = Search { e5: self, path: [0; 40], out: Vec::new(), limit };
        let h = s.c_bound(mask, eo & 0x7FF, x, t);
        for depth in h..=h + 6 {
            s.dfs_c(mask, eo & 0x7FF, x, t, depth, last, 0);
            if !s.out.is_empty() {
                break;
            }
        }
        s.out
    }

    fn d_node(&self, cube: &Cube5, classes: u32) -> DNode {
        let (mid, low, high) = slots_of(cube);
        let set = |slots: &[u8; 12]| {
            let mask = (0..12).filter(|c| (classes >> c) & 1 == 1).fold(0usize, |a, c| a | 1 << slots[c]);
            self.t3.slice_rank[mask] as usize
        };
        DNode { m: set(&mid), l: set(&low), h: set(&high), x: primary(&cube.c4.ct), t: primary(&cube.t) }
    }

    fn d_bound(&self, n: &DNode) -> usize {
        self.d[(n.m * N_SET + n.l) * N_SET + n.h]
            .max(self.d_x[centre_index(&self.rank8, n.x)])
            .max(self.d_t[centre_index(&self.rank8, n.t)]) as usize
    }

    /// The best choices of four classes for the middle slice: (lower bound, classes as a mask).
    fn d_subsets(&self, cube: &Cube5, keep: usize) -> Vec<(usize, u32)> {
        let mut all: Vec<(usize, u32)> = (0u32..4096)
            .filter(|q| q.count_ones() == 4)
            .map(|q| (self.d_bound(&self.d_node(cube, q)), q))
            .collect();
        all.sort();
        all.truncate(keep);
        all
    }

    /// Phase D for one choice of classes: solutions of the shortest length (up to `limit`)
    /// after which phase E can finish.
    fn solve_d(&self, cube: &Cube5, classes: u32, last: u8, limit: usize, extra: usize) -> Vec<Vec<u8>> {
        let node = self.d_node(cube, classes);
        let mut s = Search { e5: self, path: [0; 40], out: Vec::new(), limit };
        let h = self.d_bound(&node);
        let mut first = None;
        for depth in h..=h + 8 {
            if first.is_some_and(|f| depth > f + extra) {
                break;
            }
            if s.dfs_d(cube, &node, depth, last, 0) {
                break;
            }
            if first.is_none() && !s.out.is_empty() {
                first = Some(depth);
            }
        }
        s.out
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

    /// Phase-D2 start of a cube whose middle slice holds four whole classes,
    /// or `None` if the later phases cannot finish it.
    fn d2_node(&self, cube: &Cube5) -> Option<D2Node> {
        let (lo, hi) = self.outer_perms(cube)?;
        self.e_exact(lo, hi)?;
        let (mid, _, _) = slots_of(cube);
        let (mut ilo, mut ihi) = ([0u8; 4], [0u8; 4]);
        for s in 0..4 {
            ilo[s] = mid[(cube.c4.ep[s + 8] % 12) as usize].checked_sub(8)?;
            ihi[s] = mid[(cube.c4.ep[s + 20] % 12) as usize].checked_sub(8)?;
        }
        let (x, t) = (primary(&cube.c4.ct), primary(&cube.t));
        let side = *self.side_id.get(&((x >> 8) << 16 | (t >> 8)))? as usize;
        let n = D2Node { ilo: perm_rank(&ilo), ihi: perm_rank(&ihi), side };
        (self.d2_dist(&n) != FAR).then_some(n)
    }

    #[inline(always)]
    fn d2_dist(&self, n: &D2Node) -> u8 {
        self.d2[(n.ilo * 24 + n.ihi) * self.side_mv.len() + n.side]
    }

    /// Phase D2: every shortest solution, up to `limit`, by walking down the exact table.
    fn solve_d2(&self, cube: &Cube5, last: u8, limit: usize) -> Vec<Vec<u8>> {
        let Some(node) = self.d2_node(cube) else { return Vec::new() };
        let mut s = Search { e5: self, path: [0; 40], out: Vec::new(), limit };
        s.dfs_d2(&node, last, 0);
        s.out
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

    fn e_bound(&self, n: &ENode) -> usize {
        n.d.max(self.e_centre(n)) as usize
    }

    /// Phase E: one shortest solution.
    fn solve_e(&self, cube: &Cube5, last: u8) -> Option<Vec<u8>> {
        let node = self.e_node(cube)?;
        let mut s = Search { e5: self, path: [0; 40], out: Vec::new(), limit: 1 };
        // A start whose parts are each solvable but not together would search for ever.
        let bound = self.e_bound(&node);
        for depth in bound..=bound + 8 {
            if s.dfs_e(&node, depth, last, 0) {
                return s.out.pop();
            }
        }
        None
    }

    /// The best labellings for phase C, public for ranking staged cubes: (lower bound, labelling).
    pub fn labellings(&self, cube: &Cube5, keep: usize) -> Vec<(u8, u16)> {
        self.c_labellings(cube, keep)
    }

    /// Phases C and D from a cube with staged centres and one labelling:
    /// cubes ready for phase E, each with a lower bound on its total edge length.
    pub fn candidates(&self, cube: &Cube5, lab: u16, last: u8, p: &EdgeParams) -> Vec<Candidate> {
        let mut out = Vec::new();
        for mc in self.solve_c(cube, lab, last, p.c_keep) {
            let mut cc = *cube;
            cc.apply_seq(&mc);
            let last = mc.last().copied().unwrap_or(last);
            for (_, q) in self.d_subsets(&cc, p.subsets) {
                for md in self.solve_d(&cc, q, last, p.d_keep, p.d_extra) {
                    let mut c1 = cc;
                    c1.apply_seq(&md);
                    // Phase D2 pairs the middle slice; each way of doing it is a candidate.
                    for m2 in self.solve_d2(&c1, md.last().copied().unwrap_or(last), p.d2_keep) {
                        let mut c = c1;
                        c.apply_seq(&m2);
                        if let Some(n) = self.e_node(&c) {
                            let moves: Vec<u8> = mc.iter().chain(md.iter()).chain(m2.iter()).copied().collect();
                            out.push(Candidate { bound: moves.len() + self.e_bound(&n), len_c: mc.len(), moves, cube: c });
                        }
                    }
                }
            }
        }
        out
    }

    /// Phase E on a candidate.
    pub fn finish(&self, cand: &Candidate) -> Option<Reduction> {
        let me = self.solve_e(&cand.cube, cand.moves.last().copied().unwrap_or(255))?;
        let mut c = cand.cube;
        c.apply_seq(&me);
        debug_assert!(is_reduced(&c));
        let lens = [cand.len_c, cand.moves.len() - cand.len_c, me.len()];
        let moves: Vec<u8> = cand.moves.iter().chain(me.iter()).copied().collect();
        Some(Reduction { moves, lens, cube: c })
    }

    /// Development aid: scramble a solved cube inside each phase's own moves
    /// and check that the phase finds its way back.
    pub fn self_test(&self) {
        let mut seed = 99u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        for round in 0..4 {
            let mut c = Cube5::solved();
            for _ in 0..40 {
                c.apply(E_MOVES[rnd(14)]);
            }
            let start = self.e_node(&c).map(|n| self.e_bound(&n));
            let t = std::time::Instant::now();
            let sol = self.solve_e(&c, 255);
            eprintln!(
                "E round {}: bound {:?}, solution {:?} moves, {:.0} ms",
                round,
                start,
                sol.as_ref().map(|s| s.len()),
                t.elapsed().as_secs_f64() * 1e3
            );
            if let Some(sol) = sol {
                c.apply_seq(&sol);
                assert!(is_reduced(&c));
            }
        }
        for round in 0..4 {
            let mut c = Cube5::solved();
            for _ in 0..40 {
                c.apply(G_MOVES[rnd(20)]);
            }
            let subsets = self.d_subsets(&c, 3);
            let t = std::time::Instant::now();
            let sols = self.solve_d(&c, subsets[0].1, 255, 3, 0);
            eprintln!(
                "D round {}: bounds {:?}, {} solutions of length {:?}, {:.0} ms",
                round,
                subsets.iter().map(|x| x.0).collect::<Vec<_>>(),
                sols.len(),
                sols.first().map(|s| s.len()),
                t.elapsed().as_secs_f64() * 1e3
            );
        }
    }
}

impl Search<'_> {
    fn c_bound(&self, mask: u32, flip: usize, x: u32, t: u32) -> usize {
        let e = self.e5;
        let centres = e.c_centres[e.rank8[(x >> 16) as usize] as usize * 70 + e.rank8[(t >> 16) as usize] as usize];
        e.c_dist(mask, flip).max(centres) as usize
    }

    #[allow(clippy::too_many_arguments)]
    fn dfs_c(&mut self, mask: u32, flip: usize, x: u32, t: u32, rem: usize, last: u8, depth: usize) -> bool {
        if rem == 0 {
            if mask == LOW_HALF && flip == 0 && self.c_bound(mask, flip, x, t) == 0 {
                self.out.push(self.path[..depth].to_vec());
                return self.out.len() >= self.limit;
            }
            return false;
        }
        let e = self.e5;
        for &m in &H_MOVES {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let (a, b) = (e.bw.apply(m, mask), e.t3.flip_mv[flip][m as usize % 18] as usize);
            let (c, d) = (e.bx.apply(m, x), e.bt.apply(m, t));
            if self.c_bound(a, b, c, d) > rem - 1 {
                continue;
            }
            self.path[depth] = m;
            if self.dfs_c(a, b, c, d, rem - 1, m, depth + 1) {
                return true;
            }
        }
        false
    }

    fn dfs_d(&mut self, root: &Cube5, n: &DNode, rem: usize, last: u8, depth: usize) -> bool {
        let e = self.e5;
        if rem == 0 {
            // The slice holds four whole classes; phase E must also accept the rest.
            let mut c = *root;
            c.apply_seq(&self.path[..depth]);
            if e.d2_node(&c).is_some() {
                self.out.push(self.path[..depth].to_vec());
                return self.out.len() >= self.limit;
            }
            return false;
        }
        for (k, &m) in G_MOVES.iter().enumerate() {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let next = DNode {
                m: e.set_mv[0][n.m][k] as usize,
                l: e.set_mv[1][n.l][k] as usize,
                h: e.set_mv[2][n.h][k] as usize,
                x: e.bx.apply(m, n.x),
                t: e.bt.apply(m, n.t),
            };
            if e.d_bound(&next) > rem - 1 {
                continue;
            }
            self.path[depth] = m;
            if self.dfs_d(root, &next, rem - 1, m, depth + 1) {
                return true;
            }
        }
        false
    }

    /// Follows every move that brings the D2 table one step down.
    fn dfs_d2(&mut self, n: &D2Node, last: u8, depth: usize) -> bool {
        let e = self.e5;
        let d = e.d2_dist(n);
        if d == 0 {
            self.out.push(self.path[..depth].to_vec());
            return self.out.len() >= self.limit;
        }
        for (k, &m) in E_MOVES.iter().enumerate() {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let next = D2Node {
                ilo: e.in_low_mv[n.ilo][k] as usize,
                ihi: e.in_high_mv[n.ihi][k] as usize,
                side: e.side_mv[n.side][k] as usize,
            };
            if e.d2_dist(&next) + 1 != d {
                continue;
            }
            self.path[depth] = m;
            if self.dfs_d2(&next, m, depth + 1) {
                return true;
            }
        }
        false
    }

    fn dfs_e(&mut self, n: &ENode, rem: usize, last: u8, depth: usize) -> bool {
        if rem == 0 {
            if n.d == 0 && self.e5.e_centre(n) == 0 {
                self.out.push(self.path[..depth].to_vec());
                return true;
            }
            return false;
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
            if e.e_centre(&next) as usize > rem - 1 {
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

