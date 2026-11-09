//! Exact distance to "all centres staged": every centre on a face of its own
//! axis (R/L colours on R/L, U/D on U/D, F/B on F/B), using all moves.
//!
//! The state is the set of the 8 R/L-coloured centres (735,471 choices) and,
//! among the other 16 positions, the set of the 8 U/D-coloured ones (12,870):
//! 9,465,511,770 states, stored modulo 3 in two bits each (2.37 GB).

use crate::cube::tables as cube_tables;
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes, Ranks, P1_MOVES};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

pub const N_RL: usize = 735_471;
pub const N_UD: usize = 12_870;
pub const N_CT: usize = N_RL * N_UD;
const N_BYTES: usize = (N_CT + 3) / 4;
/// R/L colours on the R and L faces, U/D colours on the U and D faces.
pub const GOAL_RL: u32 = 0xFF_0000;
pub const GOAL_UD: u32 = 0x00_00FF;

/// Small tables that turn a pair of masks into a table index.
pub struct CtIndex {
    /// Colex rank of an 8-of-24 mask, byte by byte: `[byte position][byte][bits set below]`.
    rank24: Vec<[[u32; 9]; 256]>,
    pub unrank24: Vec<u32>,
    /// `[rl byte][ud byte]`: the ud bits at the positions the rl byte leaves free, packed.
    ext: Vec<[u8; 256]>,
    zeros: [u8; 256],
}

impl CtIndex {
    pub fn new() -> CtIndex {
        let mut binom = [[0u32; 10]; 25];
        for n in 0..25 {
            binom[n][0] = 1;
            for k in 1..10.min(n + 1) {
                binom[n][k] = binom[n - 1][k - 1] + if k <= n - 1 { binom[n - 1][k] } else { 0 };
            }
        }
        let mut rank24 = vec![[[0u32; 9]; 256]; 3];
        for k in 0..3 {
            for byte in 0..256usize {
                for below in 0..9usize {
                    let mut r = 0;
                    let mut i = below;
                    for j in 0..8 {
                        if (byte >> j) & 1 == 1 {
                            i += 1;
                            if i <= 9 {
                                r += binom[8 * k + j][i.min(9)];
                            }
                        }
                    }
                    rank24[k][byte][below] = r;
                }
            }
        }
        let mut unrank24 = Vec::with_capacity(N_RL);
        for x in 0u32..1 << 24 {
            if x.count_ones() == 8 {
                unrank24.push(x);
            }
        }
        assert_eq!(unrank24.len(), N_RL);
        let mut ext = vec![[0u8; 256]; 256];
        let mut zeros = [0u8; 256];
        for rl in 0..256usize {
            zeros[rl] = (8 - rl.count_ones()) as u8;
            for ud in 0..256usize {
                let (mut out, mut n) = (0u8, 0);
                for j in 0..8 {
                    if (rl >> j) & 1 == 0 {
                        out |= (((ud >> j) & 1) as u8) << n;
                        n += 1;
                    }
                }
                ext[rl][ud] = out;
            }
        }
        let ix = CtIndex { rank24, unrank24, ext, zeros };
        for &r in &[0usize, 1, 12345, N_RL - 1] {
            assert_eq!(ix.rank_rl(ix.unrank24[r]), r);
        }
        ix
    }

    #[inline(always)]
    pub fn rank_rl(&self, m: u32) -> usize {
        let (b0, b1, b2) = ((m & 255) as usize, ((m >> 8) & 255) as usize, (m >> 16) as usize);
        let c0 = b0.count_ones() as usize;
        let c1 = c0 + b1.count_ones() as usize;
        (self.rank24[0][b0][0] + self.rank24[1][b1][c0] + self.rank24[2][b2][c1]) as usize
    }

    /// The U/D mask restricted to the 16 positions the R/L mask leaves free.
    #[inline(always)]
    pub fn sub16(&self, rl: u32, ud: u32) -> usize {
        let (r0, r1, r2) = ((rl & 255) as usize, ((rl >> 8) & 255) as usize, (rl >> 16) as usize);
        let (u0, u1, u2) = ((ud & 255) as usize, ((ud >> 8) & 255) as usize, (ud >> 16) as usize);
        let (z0, z1) = (self.zeros[r0] as usize, self.zeros[r1] as usize);
        self.ext[r0][u0] as usize | (self.ext[r1][u1] as usize) << z0 | (self.ext[r2][u2] as usize) << (z0 + z1)
    }
}

impl Default for CtIndex {
    fn default() -> CtIndex {
        CtIndex::new()
    }
}

pub struct Ctab {
    pub ix: CtIndex,
    data: Bytes,
}

#[inline(always)]
fn get(bytes: &[AtomicU8], idx: usize) -> u8 {
    (bytes[idx >> 2].load(Relaxed) >> ((idx & 3) * 2)) & 3
}

/// Sets an unvisited entry (3) to `v`; returns true if this call did it.
#[inline(always)]
fn set(bytes: &[AtomicU8], idx: usize, v: u8) -> bool {
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
}

/// Spreads the 16 bits of `sub` over the positions `rl` leaves free.
fn deposit(rl: u32, sub: u32) -> u32 {
    let (mut out, mut n) = (0u32, 0);
    for p in 0..24 {
        if (rl >> p) & 1 == 0 {
            out |= ((sub >> n) & 1) << p;
            n += 1;
        }
    }
    out
}

fn generate(ix: &CtIndex, bp: &BitPerm, ranks: &Ranks) -> Vec<u8> {
    let ct = cube_tables();
    let bytes: Vec<AtomicU8> = {
        let mut v = vec![0xFFu8; N_BYTES];
        let p = v.as_mut_ptr() as *mut AtomicU8;
        let (len, cap) = (v.len(), v.capacity());
        std::mem::forget(v);
        // AtomicU8 has the same layout as u8.
        unsafe { Vec::from_raw_parts(p, len, cap) }
    };
    let index = |rl: u32, ud: u32| ix.rank_rl(rl) * N_UD + ranks.rank16[ix.sub16(rl, ud)] as usize;
    let goal = index(GOAL_RL, GOAL_UD);
    set(&bytes, goal, 0);
    let mut done = 1u64;
    let mut depth = 0usize;
    // Push from explicit frontiers while they are small.
    // The last push starts from a frontier too large to keep its successors in a list.
    let mut frontier: Vec<u64> = vec![goal as u64];
    while !frontier.is_empty() {
        let v = ((depth + 1) % 3) as u8;
        let keep = frontier.len() <= 5_000_000;
        let next: Vec<(Vec<u64>, u64)> = frontier
            .par_chunks(4096)
            .map(|chunk| {
                let mut out = Vec::new();
                let mut n = 0u64;
                for &x in chunk {
                    let rl = ix.unrank24[x as usize / N_UD];
                    let ud = deposit(rl, ranks.unrank16[x as usize % N_UD] as u32);
                    for &m in &P1_MOVES {
                        let y = index(bp.apply(m, rl), bp.apply(m, ud));
                        if set(&bytes, y, v) {
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
        eprintln!("  centre table: depth {:2}  {:>11} new  {:>11} total", depth, n, done);
        if !keep {
            break;
        }
    }
    drop(frontier);
    // Then pull: every unvisited state looks for a neighbour at the current depth.
    // Where each move sends each centre position.
    let mut dst = [[0u8; 24]; 27];
    for (k, &m) in P1_MOVES.iter().enumerate() {
        for q in 0..24 {
            dst[k][ct.ct_src[m as usize][q] as usize] = q as u8;
        }
    }
    while done < N_CT as u64 {
        let target = (depth % 3) as u8;
        let v = ((depth + 1) % 3) as u8;
        let n: u64 = (0..N_RL)
            .into_par_iter()
            .with_min_len(16)
            .map(|b| {
                let base = b * N_UD;
                let mut unv: Vec<u16> = Vec::new();
                for r in 0..N_UD {
                    if get(&bytes, base + r) == 3 {
                        unv.push(r as u16);
                    }
                }
                if unv.is_empty() {
                    return 0;
                }
                let rl = ix.unrank24[b];
                let mut free = [0usize; 16];
                let mut n = 0;
                for p in 0..24 {
                    if (rl >> p) & 1 == 0 {
                        free[n] = p;
                        n += 1;
                    }
                }
                let mut found = 0u64;
                for (k, &m) in P1_MOVES.iter().enumerate() {
                    let rl2 = bp.apply(m, rl);
                    let nb = ix.rank_rl(rl2) * N_UD;
                    // Index of each position among the positions `rl2` leaves free.
                    let mut slot = [0u16; 24];
                    let mut c = 0;
                    for p in 0..24 {
                        slot[p] = c;
                        if (rl2 >> p) & 1 == 0 {
                            c += 1;
                        }
                    }
                    // The move permutes the 16 free positions; apply it to a mask byte by byte.
                    let mut lo = [0u16; 256];
                    let mut hi = [0u16; 256];
                    for x in 1..256usize {
                        let j = x.trailing_zeros() as usize;
                        lo[x] = lo[x & (x - 1)] | 1 << slot[dst[k][free[j]] as usize];
                        hi[x] = hi[x & (x - 1)] | 1 << slot[dst[k][free[j + 8]] as usize];
                    }
                    let mut i = 0;
                    while i < unv.len() {
                        let sub = ranks.unrank16[unv[i] as usize] as usize;
                        let sub2 = lo[sub & 255] | hi[sub >> 8];
                        if get(&bytes, nb + ranks.rank16[sub2 as usize] as usize) == target {
                            set(&bytes, base + unv[i] as usize, v);
                            found += 1;
                            unv.swap_remove(i);
                        } else {
                            i += 1;
                        }
                    }
                    if unv.is_empty() {
                        break;
                    }
                }
                found
            })
            .sum();
        assert!(n > 0, "centre table: unreachable states remain");
        depth += 1;
        done += n;
        eprintln!("  centre table: depth {:2}  {:>11} new  {:>11} total", depth, n, done);
    }
    let mut v = std::mem::ManuallyDrop::new(bytes);
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

impl Ctab {
    pub fn load(bp: &BitPerm, ranks: &Ranks) -> Ctab {
        let ix = CtIndex::new();
        let data = load_or_gen(&table_dir().join("centres-exact.bin"), N_BYTES, || generate(&ix, bp, ranks));
        Ctab { ix, data }
    }

    #[inline(always)]
    pub fn index(&self, ranks: &Ranks, rl: u32, ud: u32) -> usize {
        self.ix.rank_rl(rl) * N_UD + ranks.rank16[self.ix.sub16(rl, ud)] as usize
    }

    /// Distance modulo 3.
    #[inline(always)]
    pub fn get(&self, idx: usize) -> u8 {
        (self.data[idx >> 2] >> ((idx & 3) * 2)) & 3
    }

    /// Exact distance, found by walking down to the goal.
    pub fn exact(&self, bp: &BitPerm, ranks: &Ranks, mut rl: u32, mut ud: u32) -> u8 {
        let mut d = 0u8;
        let mut cur = self.get(self.index(ranks, rl, ud));
        while (rl, ud) != (GOAL_RL, GOAL_UD) {
            let want = (cur + 2) % 3;
            let mut moved = false;
            for &m in &P1_MOVES {
                let (a, b) = (bp.apply(m, rl), bp.apply(m, ud));
                if self.get(self.index(ranks, a, b)) == want {
                    (rl, ud) = (a, b);
                    cur = want;
                    d += 1;
                    moved = true;
                    break;
                }
            }
            assert!(moved, "corrupt centre table");
        }
        d
    }
}
