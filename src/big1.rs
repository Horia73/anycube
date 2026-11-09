//! Exact distance to the 3x3x3 "domino" subgroup <U, D, R2, L2, F2, B2>
//! for every (twist, flip, slice) coordinate: 2,217,093,120 entries, stored
//! modulo 3 in two bits each (554 MB). Used along all three axes as the
//! lower bound of the exact 3x3x3 search, as in Kociemba's optimal solver.

use crate::c3::{T3, N_FLIP, N_SLICE, N_TWIST};
use crate::tables::{load_or_gen, table_dir, Bytes};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

pub const N_BIG: usize = N_TWIST * N_FLIP * N_SLICE;
/// States at each distance (Kociemba's phase-1 distribution).
const EXPECTED: [u64; 13] = [
    1, 4, 50, 592, 7156, 87236, 1043817, 12070278, 124946368, 821605960, 1199128738, 58202444, 476,
];

pub struct Big1 {
    data: Bytes,
}

#[inline(always)]
fn get(bytes: &[AtomicU8], idx: usize) -> u8 {
    (bytes[idx >> 2].load(Relaxed) >> ((idx & 3) * 2)) & 3
}

/// Sets an unvisited entry (3) to `v`; returns true if this call did it.
/// Entries that are already set are left alone.
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

fn generate(t: &T3) -> Vec<u8> {
    let bytes: Vec<AtomicU8> = (0..N_BIG / 4).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    let goal = t.slice_goal();
    set(&bytes, goal, 0);
    let mut counts = vec![1u64];
    // Push from explicit frontiers while they are small.
    let mut frontier: Vec<u32> = vec![goal as u32];
    const PUSH: usize = 8;
    for depth in 0..PUSH {
        let v = ((depth + 1) % 3) as u8;
        let keep = depth + 1 < PUSH;
        let parts: Vec<(Vec<u32>, u64)> = frontier
            .par_chunks(1024)
            .map(|chunk| {
                let mut out = Vec::new();
                let mut n = 0u64;
                for &x in chunk {
                    let x = x as usize;
                    let (sl, b) = (x % N_SLICE, x / N_SLICE);
                    let (fl, tw) = (b % N_FLIP, b / N_FLIP);
                    for m in 0..18 {
                        let y = (t.twist_mv[tw][m] as usize * N_FLIP + t.flip_mv[fl][m] as usize)
                            * N_SLICE
                            + t.slice_mv[sl][m] as usize;
                        if set(&bytes, y, v) {
                            n += 1;
                            if keep {
                                out.push(y as u32);
                            }
                        }
                    }
                }
                (out, n)
            })
            .collect();
        counts.push(parts.iter().map(|p| p.1).sum());
        frontier = parts.into_iter().flat_map(|p| p.0).collect();
        eprintln!("  3x3x3 table: depth {:2}  {:>11}", depth + 1, counts[depth + 1]);
    }
    drop(frontier);
    // Then pull: every unvisited state looks for a neighbour at the current depth.
    let mut depth = PUSH;
    loop {
        let target = (depth % 3) as u8;
        let v = ((depth + 1) % 3) as u8;
        let n: u64 = (0..N_TWIST * N_FLIP)
            .into_par_iter()
            .with_min_len(256)
            .map(|b| {
                let base = b * N_SLICE;
                let mut unv = [0u16; N_SLICE];
                let mut k = 0;
                for sl in 0..N_SLICE {
                    if get(&bytes, base + sl) == 3 {
                        unv[k] = sl as u16;
                        k += 1;
                    }
                }
                if k == 0 {
                    return 0;
                }
                let (fl, tw) = (b % N_FLIP, b / N_FLIP);
                let mut found = 0u64;
                for m in 0..18 {
                    let nb =
                        (t.twist_mv[tw][m] as usize * N_FLIP + t.flip_mv[fl][m] as usize) * N_SLICE;
                    let mut i = 0;
                    while i < k {
                        let sl = unv[i] as usize;
                        if get(&bytes, nb + t.slice_mv[sl][m] as usize) == target {
                            set(&bytes, base + sl, v);
                            found += 1;
                            k -= 1;
                            unv[i] = unv[k];
                        } else {
                            i += 1;
                        }
                    }
                    if k == 0 {
                        break;
                    }
                }
                found
            })
            .sum();
        if n == 0 {
            break;
        }
        counts.push(n);
        depth += 1;
        eprintln!("  3x3x3 table: depth {:2}  {:>11}", depth, n);
    }
    assert_eq!(counts, EXPECTED, "distance distribution differs from the known one");
    let mut v = std::mem::ManuallyDrop::new(bytes);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

impl Big1 {
    pub fn load(t: &T3) -> Big1 {
        Big1 { data: load_or_gen(&table_dir().join("phase1-exact.bin"), N_BIG / 4, || generate(t)) }
    }

    /// Distance modulo 3.
    #[inline(always)]
    pub fn get(&self, idx: usize) -> u8 {
        (self.data[idx >> 2] >> ((idx & 3) * 2)) & 3
    }

    #[inline(always)]
    pub fn index(tw: usize, fl: usize, sl: usize) -> usize {
        (tw * N_FLIP + fl) * N_SLICE + sl
    }

    /// Exact distance, found by walking down to the goal.
    pub fn exact(&self, t: &T3, mut tw: usize, mut fl: usize, mut sl: usize) -> u8 {
        let goal = t.slice_goal();
        let mut d = 0u8;
        let mut cur = self.get(Self::index(tw, fl, sl));
        while Self::index(tw, fl, sl) != goal {
            let want = (cur + 2) % 3;
            let mut moved = false;
            for m in 0..18 {
                let (a, b, c) =
                    (t.twist_mv[tw][m] as usize, t.flip_mv[fl][m] as usize, t.slice_mv[sl][m] as usize);
                if self.get(Self::index(a, b, c)) == want {
                    (tw, fl, sl) = (a, b, c);
                    cur = want;
                    d += 1;
                    moved = true;
                    break;
                }
            }
            assert!(moved, "corrupt 3x3x3 table");
        }
        d
    }
}
