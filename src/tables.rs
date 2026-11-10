//! Coordinates and pruning tables for the three reduction phases.
//!
//! Phase 1 (27 moves): bring the R/L-coloured centres onto the R and L faces.
//! Phase 2 (23 moves): separate U/D from F/B centres, bring the R/L centres
//!   into a pattern phase 3 can finish, make the wing permutation even.
//! Phase 3 (17 moves): pair the 12 dedges and solve the centres.
//!
//! Centre patterns are 24-bit masks permuted by byte-sliced lookup tables.

use crate::cube::{tables as cube_tables, Cube};
use memmap2::Mmap;
use rayon::prelude::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

pub const P1_MOVES: [u8; 27] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26,
];
pub const P2_MOVES: [u8; 23] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 21, 22, 23, 25];
pub const P3_MOVES: [u8; 17] = [0, 1, 2, 4, 6, 7, 8, 9, 10, 11, 13, 15, 16, 17, 19, 22, 25];

pub const P1_GOAL: u32 = 0xFF_0000;
pub const N_SIGMA: usize = 239_500_800; // 12! / 2
const N_P2: usize = 12870 * 70 * 2;
const N_CT3: usize = 70 * 70 * 70 * 2;
pub const UNREACHED: u8 = 255;

/// Permutes 24-bit masks the way each move permutes the centres.
pub struct BitPerm {
    t: Vec<[[u32; 256]; 3]>,
}

impl BitPerm {
    fn new() -> BitPerm {
        BitPerm::from_src(&cube_tables().ct_src)
    }

    /// For any orbit of 24 positions: `src[m][i]` is the position whose piece
    /// move `m` brings to position `i`.
    pub fn from_src(src: &[[u8; 24]]) -> BitPerm {
        let mut t = vec![[[0u32; 256]; 3]; src.len()];
        for m in 0..src.len() {
            let src = &src[m];
            for k in 0..3 {
                for byte in 0..256usize {
                    let mut out = 0u32;
                    for i in 0..24 {
                        let s = src[i] as usize;
                        if s / 8 == k && (byte >> (s % 8)) & 1 == 1 {
                            out |= 1 << i;
                        }
                    }
                    t[m][k][byte] = out;
                }
            }
        }
        BitPerm { t }
    }

    #[inline(always)]
    pub fn apply(&self, m: u8, x: u32) -> u32 {
        let t = &self.t[m as usize];
        t[0][(x & 255) as usize] | t[1][((x >> 8) & 255) as usize] | t[2][(x >> 16) as usize]
    }
}

/// Rank tables for 4-of-8 and 8-of-16 bit masks.
pub struct Ranks {
    pub rank8: [u8; 256],
    pub unrank8: [u8; 70],
    pub rank16: Vec<u16>,
    pub unrank16: Vec<u16>,
}

impl Ranks {
    pub fn new() -> Ranks {
        let mut rank8 = [255u8; 256];
        let mut unrank8 = [0u8; 70];
        let mut n = 0;
        for x in 0..256usize {
            if x.count_ones() == 4 {
                rank8[x] = n as u8;
                unrank8[n] = x as u8;
                n += 1;
            }
        }
        assert_eq!(n, 70);
        let mut rank16 = vec![u16::MAX; 65536];
        let mut unrank16 = vec![0u16; 12870];
        let mut n = 0;
        for x in 0..65536usize {
            if x.count_ones() == 8 {
                rank16[x] = n as u16;
                unrank16[n] = x as u16;
                n += 1;
            }
        }
        assert_eq!(n, 12870);
        Ranks { rank8, unrank8, rank16, unrank16 }
    }
}

pub enum Bytes {
    Owned(Vec<u8>),
    Mapped(Mmap),
}

impl std::ops::Deref for Bytes {
    type Target = [u8];
    #[inline(always)]
    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Owned(v) => v,
            Bytes::Mapped(m) => m,
        }
    }
}

pub struct Tabs {
    pub bp: BitPerm,
    pub ranks: Ranks,
    /// Phase 1 distance, indexed by the 24-bit mask of R/L-coloured centres.
    pub p1: Bytes,
    /// The same for the U/D colours onto U/D and the F/B colours onto F/B.
    pub stage: [Bytes; 2],
    /// Phase 2 distance over (U/D-vs-F/B pattern, R-vs-L pattern, wing parity).
    pub p2: Bytes,
    /// Phase 3 centre distance over (U, F, R patterns, parity).
    pub ct3: Bytes,
    /// Phase 3 pairing distance over the dedge pairing permutation.
    pub sigma: Bytes,
    /// Per phase-3 move: slot permutation of the low and (inverted) high wings.
    pub pl: [[u8; 12]; 17],
    pub phinv: [[u8; 12]; 17],
}

// ---------- phase 1 ----------

pub fn p1_mask(c: &Cube) -> u32 {
    let mut m = 0;
    for i in 0..24 {
        if c.ct[i] % 3 == 1 {
            m |= 1 << i;
        }
    }
    m
}

/// Masks of the U/D-, F/B- and R/L-coloured centres.
pub fn axis_masks(c: &Cube) -> [u32; 3] {
    let mut m = [0u32; 3];
    for i in 0..24 {
        m[[0, 2, 1][(c.ct[i] % 3) as usize]] |= 1 << i;
    }
    m
}

/// Distance to bring the eight centres of one colour pair onto the face pair `goal`.
fn gen_p1(bp: &BitPerm, goal: u32) -> Vec<u8> {
    gen_stage(bp, goal, &P1_MOVES)
}

/// The same for any orbit and move set.
pub fn gen_stage(bp: &BitPerm, goal: u32, moves: &[u8]) -> Vec<u8> {
    let mut dist = vec![UNREACHED; 1 << 24];
    dist[goal as usize] = 0;
    let mut frontier = vec![goal];
    let mut depth = 0u8;
    let mut total = 1usize;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &x in &frontier {
            for &m in moves {
                let y = bp.apply(m, x);
                if dist[y as usize] == UNREACHED {
                    dist[y as usize] = depth + 1;
                    next.push(y);
                }
            }
        }
        total += next.len();
        frontier = next;
        depth += 1;
    }
    assert_eq!(total, 735_471);
    dist
}

// ---------- phase 3 centres ----------

/// Mask of centres showing the first colour of their axis (U, F or R).
pub fn ct3_mask(c: &Cube) -> u32 {
    let mut m = 0;
    for i in 0..24 {
        if c.ct[i] < 3 {
            m |= 1 << i;
        }
    }
    m
}

/// Parity of the dedge arrangement (seen through the high wings) against the corners.
pub fn ct3_parity(c: &Cube) -> u8 {
    let mut high = [0u8; 12];
    for s in 0..12 {
        high[s] = c.ep[s + 12] % 12;
    }
    crate::cube::perm_parity(&high) ^ c.corner_parity()
}

impl Tabs {
    /// Index into `ct3`. Each face pair must hold four of each of its two colours.
    #[inline(always)]
    pub fn ct3_index(&self, mask: u32, parity: u8) -> usize {
        let r = &self.ranks.rank8;
        let (a, b, c) = (
            r[(mask & 255) as usize] as usize,
            r[((mask >> 8) & 255) as usize] as usize,
            r[(mask >> 16) as usize] as usize,
        );
        ((a * 70 + b) * 70 + c) * 2 + parity as usize
    }

    #[inline(always)]
    pub fn p2_index(&self, mask: u32, parity: u8) -> usize {
        let a = self.ranks.rank16[(mask & 0xFFFF) as usize] as usize;
        let b = self.ranks.rank8[(mask >> 16) as usize] as usize;
        (a * 70 + b) * 2 + parity as usize
    }
}

/// True for the moves phase 3 may use (in any of the 36 move names).
#[inline(always)]
pub fn is_p3_move(m: u8) -> bool {
    if m >= 18 {
        return m % 3 == 1;
    }
    !matches!(m / 3, 1 | 4) || m % 3 == 1
}

/// True for the wide half turns, which change the phase 3 parity.
#[inline(always)]
pub fn p3_flips_parity(m: u8) -> bool {
    m >= 18
}

/// True for the inner-slice quarter turns, which change the wing parity.
#[inline(always)]
pub fn p2_flips_parity(m: u8) -> bool {
    m >= 18 && m % 3 != 1
}

fn gen_ct3(bp: &BitPerm, ranks: &Ranks) -> Vec<u8> {
    let idx = |mask: u32, p: u32| -> usize {
        let r = &ranks.rank8;
        let (a, b, c) = (
            r[(mask & 255) as usize] as usize,
            r[((mask >> 8) & 255) as usize] as usize,
            r[(mask >> 16) as usize] as usize,
        );
        ((a * 70 + b) * 70 + c) * 2 + p as usize
    };
    let mut dist = vec![UNREACHED; N_CT3];
    let mut frontier = Vec::new();
    // Goals: the solved cube in the four orientations that keep every axis.
    for r in &cube_tables().rots {
        let ct = &r.state.ct;
        if ct[0] % 3 == 0 && ct[8] % 3 == 2 && ct[16] % 3 == 1 {
            let m = ct3_mask(&r.state);
            assert_eq!(ct3_parity(&r.state), 0);
            if dist[idx(m, 0)] == UNREACHED {
                dist[idx(m, 0)] = 0;
                frontier.push((m, 0u32));
            }
        }
    }
    assert_eq!(frontier.len(), 4);
    let mut depth = 0u8;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &(x, p) in &frontier {
            for &m in &P3_MOVES {
                let y = bp.apply(m, x);
                let q = p ^ p3_flips_parity(m) as u32;
                let i = idx(y, q);
                if dist[i] == UNREACHED {
                    dist[i] = depth + 1;
                    next.push((y, q));
                }
            }
        }
        frontier = next;
        depth += 1;
    }
    dist
}

// ---------- phase 2 ----------

/// Low 16 bits: U/D-coloured centres on the U, D, F, B faces.
/// High 8 bits: R-coloured centres on the R, L faces.
pub fn p2_mask(c: &Cube) -> u32 {
    let mut m = 0;
    for i in 0..16 {
        if c.ct[i] % 3 == 0 {
            m |= 1 << i;
        }
    }
    for i in 16..24 {
        if c.ct[i] == 1 {
            m |= 1 << i;
        }
    }
    m
}

fn gen_p2(bp: &BitPerm, ranks: &Ranks, ct3: &[u8]) -> Vec<u8> {
    // R/L patterns from which phase 3 can finish the centres.
    let mut rl_ok = [false; 70];
    for (i, &d) in ct3.iter().enumerate() {
        if d != UNREACHED {
            rl_ok[(i / 2) % 70] = true;
        }
    }
    let idx = |mask: u32, p: u32| -> usize {
        let a = ranks.rank16[(mask & 0xFFFF) as usize] as usize;
        let b = ranks.rank8[(mask >> 16) as usize] as usize;
        (a * 70 + b) * 2 + p as usize
    };
    let mut dist = vec![UNREACHED; N_P2];
    let mut frontier = Vec::new();
    for rl in 0..70 {
        if rl_ok[rl] {
            let m = 0x00FFu32 | (ranks.unrank8[rl] as u32) << 16;
            dist[idx(m, 0)] = 0;
            frontier.push((m, 0u32));
        }
    }
    let mut depth = 0u8;
    let mut total = frontier.len();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &(x, p) in &frontier {
            for &m in &P2_MOVES {
                let y = bp.apply(m, x);
                let q = p ^ p2_flips_parity(m) as u32;
                let i = idx(y, q);
                if dist[i] == UNREACHED {
                    dist[i] = depth + 1;
                    next.push((y, q));
                }
            }
        }
        total += next.len();
        frontier = next;
        depth += 1;
    }
    assert_eq!(total, N_P2);
    dist
}

// ---------- phase 3 pairing ----------

/// Wings 0..12 ("low") and 12..24 ("high") of the same dedge are `s` and `s + 12`.
/// True when every dedge class has one wing in each half and the number of
/// swapped classes is even, i.e. phase 3 can pair the dedges into a valid 3x3x3.
pub fn edges_pairable(c: &Cube) -> bool {
    wings_pairable(&c.ep)
}

/// `edges_pairable` on the wing permutation alone.
#[inline(always)]
pub fn wings_pairable(ep: &[u8; 24]) -> bool {
    let mut seen = 0u32;
    let mut flips = 0u8;
    for s in 0..12 {
        seen |= 1 << (ep[s] % 12);
        flips ^= ep[s] / 12;
    }
    seen == 0xFFF && flips == 0
}

/// Pairing permutation: `sigma[s]` is the slot whose high wing matches the
/// low wing of slot `s`. Identity means all dedges are paired.
/// Only meaningful when `edges_pairable` holds.
pub fn sigma_of(c: &Cube) -> [u8; 12] {
    let mut pos_high = [0u8; 12];
    for s in 0..12 {
        pos_high[(c.ep[s + 12] % 12) as usize] = s as u8;
    }
    let mut sg = [0u8; 12];
    for s in 0..12 {
        sg[s] = pos_high[(c.ep[s] % 12) as usize];
    }
    sg
}

/// Rank of the first ten entries (the last two follow from evenness).
#[inline(always)]
pub fn sigma_rank(s: &[u8; 12]) -> usize {
    let mut val: u64 = 0xba98_7654_3210;
    let mut idx: u32 = 0;
    for i in 0..10 {
        let v = (s[i] as u32) << 2;
        idx = idx * (12 - i as u32) + ((val >> v) & 0xf) as u32;
        val = val.wrapping_sub(0x1111_1111_1110u64 << v);
    }
    idx as usize
}

pub fn sigma_unrank(mut idx: usize) -> [u8; 12] {
    let mut digits = [0usize; 10];
    for i in (0..10).rev() {
        digits[i] = idx % (12 - i);
        idx /= 12 - i;
    }
    let mut avail: Vec<u8> = (0..12).collect();
    let mut s = [0u8; 12];
    for i in 0..10 {
        s[i] = avail.remove(digits[i]);
    }
    s[10] = avail[0];
    s[11] = avail[1];
    if crate::cube::perm_parity(&s) == 1 {
        s.swap(10, 11);
    }
    s
}

#[inline(always)]
pub fn sigma_move(pl: &[u8; 12], phinv: &[u8; 12], s: &[u8; 12]) -> [u8; 12] {
    let mut out = [0u8; 12];
    for i in 0..12 {
        out[i] = phinv[s[pl[i] as usize] as usize];
    }
    out
}

fn sigma_perms() -> ([[u8; 12]; 17], [[u8; 12]; 17]) {
    let ct = cube_tables();
    let mut pl = [[0u8; 12]; 17];
    let mut phinv = [[0u8; 12]; 17];
    for (k, &m) in P3_MOVES.iter().enumerate() {
        let src = &ct.ep_src[m as usize];
        for s in 0..12 {
            assert!(src[s] < 12 && src[s + 12] >= 12, "phase 3 moves keep wing halves");
            pl[k][s] = src[s];
            phinv[k][(src[s + 12] - 12) as usize] = s as u8;
        }
    }
    (pl, phinv)
}

fn gen_sigma(pl: &[[u8; 12]; 17], phinv: &[[u8; 12]; 17]) -> Vec<u8> {
    let dist: Vec<AtomicU8> = {
        let mut v = vec![UNREACHED; N_SIGMA];
        let p = v.as_mut_ptr() as *mut AtomicU8;
        let (len, cap) = (v.len(), v.capacity());
        std::mem::forget(v);
        // AtomicU8 has the same layout as u8.
        unsafe { Vec::from_raw_parts(p, len, cap) }
    };
    let id: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    dist[sigma_rank(&id)].store(0, Relaxed);
    let mut depth = 0u8;
    let mut done = 1u64;
    loop {
        let forward = depth < 9;
        let cnt: u64 = (0..N_SIGMA)
            .into_par_iter()
            .with_min_len(1 << 14)
            .map(|i| {
                let d = dist[i].load(Relaxed);
                if forward {
                    if d != depth {
                        return 0;
                    }
                    let s = sigma_unrank(i);
                    let mut c = 0;
                    for k in 0..17 {
                        let j = sigma_rank(&sigma_move(&pl[k], &phinv[k], &s));
                        if dist[j].load(Relaxed) == UNREACHED
                            && dist[j]
                                .compare_exchange(UNREACHED, depth + 1, Relaxed, Relaxed)
                                .is_ok()
                        {
                            c += 1;
                        }
                    }
                    c
                } else {
                    if d != UNREACHED {
                        return 0;
                    }
                    let s = sigma_unrank(i);
                    for k in 0..17 {
                        let j = sigma_rank(&sigma_move(&pl[k], &phinv[k], &s));
                        if dist[j].load(Relaxed) == depth {
                            dist[i].store(depth + 1, Relaxed);
                            return 1;
                        }
                    }
                    0
                }
            })
            .sum();
        if cnt == 0 {
            break;
        }
        done += cnt;
        depth += 1;
        eprintln!("  pairing table: depth {:2}  {:>11} new  {:>11} total", depth, cnt, done);
    }
    assert_eq!(done as usize, N_SIGMA, "every even pairing permutation must be reachable");
    let mut v = std::mem::ManuallyDrop::new(dist);
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

pub(crate) fn table_dir() -> PathBuf {
    std::env::var_os("CUBE444_TABLES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("tables"))
}

pub(crate) fn load_or_gen_quiet(path: &Path, len: usize, gen: &dyn Fn() -> Vec<u8>) -> Bytes {
    load_or_gen_inner(path, len, gen, false)
}

pub(crate) fn load_or_gen(path: &Path, len: usize, gen: impl FnOnce() -> Vec<u8>) -> Bytes {
    load_or_gen_inner(path, len, gen, true)
}

fn load_or_gen_inner(path: &Path, len: usize, gen: impl FnOnce() -> Vec<u8>, announce: bool) -> Bytes {
    if let Ok(f) = fs::File::open(path) {
        if f.metadata().map(|m| m.len() as usize == len).unwrap_or(false) {
            // Read-only private mapping of a file this program wrote.
            if let Ok(m) = unsafe { Mmap::map(&f) } {
                return Bytes::Mapped(m);
            }
        }
    }
    if announce {
        eprintln!("generating {} ...", path.display());
    }
    let v = gen();
    assert_eq!(v.len(), len);
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    let written = fs::File::create(&tmp).and_then(|mut f| f.write_all(&v)).is_ok();
    if !written || fs::rename(&tmp, path).is_err() {
        eprintln!("warning: could not cache {}", path.display());
    }
    Bytes::Owned(v)
}

impl Tabs {
    pub fn load() -> Tabs {
        let bp = BitPerm::new();
        let ranks = Ranks::new();
        // Small tables are saved too: rebuilding them costs a quarter of a second per start.
        let dir = table_dir();
        let small = |name: &str, len: usize, gen: &dyn Fn() -> Vec<u8>| load_or_gen_quiet(&dir.join(name), len, gen);
        let p1 = small("stage-rl.bin", 1 << 24, &|| gen_p1(&bp, P1_GOAL));
        let stage = [
            small("stage-ud.bin", 1 << 24, &|| gen_p1(&bp, 0x0000FF)),
            small("stage-fb.bin", 1 << 24, &|| gen_p1(&bp, 0x00FF00)),
        ];
        let ct3 = small("phase3-centres.bin", N_CT3, &|| gen_ct3(&bp, &ranks));
        let p2 = small("phase2.bin", N_P2, &|| gen_p2(&bp, &ranks, &ct3));
        let (pl, phinv) = sigma_perms();
        let sigma = load_or_gen(&table_dir().join("pairing.bin"), N_SIGMA, || gen_sigma(&pl, &phinv));
        Tabs { bp, ranks, p1, stage, p2, ct3, sigma, pl, phinv }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigma_rank_roundtrip() {
        for &i in &[0usize, 1, 2, 12345, 239_500_799, 100_000_007] {
            let s = sigma_unrank(i);
            assert_eq!(crate::cube::perm_parity(&s), 0);
            assert_eq!(sigma_rank(&s), i);
        }
    }

    #[test]
    fn sigma_tracks_the_cube() {
        let (pl, phinv) = sigma_perms();
        let mut c = Cube::solved();
        let mut s = sigma_of(&c);
        let seq = [14usize, 0, 15, 4, 16, 9, 3, 14, 11, 16, 2, 15, 7, 10];
        for &k in &seq {
            c.apply(P3_MOVES[k]);
            s = sigma_move(&pl[k], &phinv[k], &s);
            assert!(edges_pairable(&c));
            assert_eq!(s, sigma_of(&c));
        }
        assert_ne!(s, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn masks_track_the_cube() {
        let bp = BitPerm::new();
        let mut c = Cube::solved();
        let mut m = p1_mask(&c);
        assert_eq!(m, P1_GOAL);
        for &mv in &[18u8, 3, 26, 7, 21, 35, 14, 30, 2] {
            c.apply(mv);
            m = bp.apply(mv, m);
            assert_eq!(m, p1_mask(&c));
        }
    }
}
