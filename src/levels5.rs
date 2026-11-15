//! Experiment: how many cubes are d moves from a goal of the 5x5x5, level by
//! level. Not part of any mode.
//!
//! The bounds in `bench/RESULTS.md` divide the logarithm of the positions by
//! the logarithm of the growth of the move sequences, which takes every
//! sequence for a different cube. Near a goal most are not: moves that keep
//! the goal do nothing there, and different moves give the same cube. This
//! counts what there is.
//!
//! Two goals, both with all 36 moves: the centres staged, and the cube as
//! phase C leaves it for one labelling of the wings (centres staged in a
//! pattern the later phases can finish, the wing halves apart, the middle
//! edges unflipped, the wing parity even).
//!
//! The first levels are made whole. A few more are estimated from random
//! descents: from a cube of the last whole level one child a level deeper is
//! taken at random, and so on; each step multiplies a weight by the number
//! of such children and divides it by the number of parents the chosen child
//! has, so that the mean weight after j steps is the size of level j past
//! the whole ones. How deep a cube is, a few moves past the whole levels, is
//! found by trying every path back to them.

use crate::c3::T3;
use crate::c5::{tables5, Cube5};
use crate::cube::{skip_move, tables as cube_tables};
use crate::edges5::G_MOVES;
use crate::tables::BitPerm;
use rayon::prelude::*;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Mutex;

const SHARDS: usize = 1024;

/// A set of cubes with the 36 moves on it and a goal.
pub trait Space: Sync {
    type S: Copy + Send + Sync;
    fn step(&self, s: &Self::S, m: u8) -> Self::S;
    /// 64 bits that stand for the cube.
    fn key(&self, s: &Self::S) -> u64;
    fn goals(&self) -> Vec<Self::S>;
    /// The cubes there are in all.
    fn size(&self) -> f64;
}

#[inline(always)]
pub(crate) fn mix(words: &[u64]) -> u64 {
    let mut h = 0x9E37_79B9_7F4A_7C15u64;
    for &w in words {
        h = (h ^ w).wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        h ^= h >> 33;
    }
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^ (h >> 33)
}

/// Whether a move changes the parity of the wings: the wide quarter turns.
#[inline(always)]
fn odd_move(m: u8) -> bool {
    m >= 18 && m % 3 != 1
}

/// The centres as far as staging goes: where the U/D and the R/L centres of
/// each orbit are, and the parity of the wings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CState {
    x: [u32; 2],
    t: [u32; 2],
    odd: bool,
}

pub struct Centres {
    bx: BitPerm,
    bt: BitPerm,
}

impl Centres {
    pub fn new() -> Centres {
        Centres { bx: BitPerm::from_src(&cube_tables().ct_src), bt: BitPerm::from_src(&tables5().t_src) }
    }
}

impl Default for Centres {
    fn default() -> Centres {
        Centres::new()
    }
}

impl Space for Centres {
    type S = CState;

    #[inline(always)]
    fn step(&self, s: &CState, m: u8) -> CState {
        CState {
            x: [self.bx.apply(m, s.x[0]), self.bx.apply(m, s.x[1])],
            t: [self.bt.apply(m, s.t[0]), self.bt.apply(m, s.t[1])],
            odd: s.odd ^ odd_move(m),
        }
    }

    #[inline(always)]
    fn key(&self, s: &CState) -> u64 {
        mix(&[s.x[0] as u64 | (s.x[1] as u64) << 24 | (s.odd as u64) << 48, s.t[0] as u64 | (s.t[1] as u64) << 24])
    }

    fn goals(&self) -> Vec<CState> {
        vec![CState { x: [0x00_00FF, 0xFF_0000], t: [0x00_00FF, 0xFF_0000], odd: false }]
    }

    fn size(&self) -> f64 {
        let orbit = 735_471.0 * 12_870.0; // 8 of 24, then 8 of 16
        orbit * orbit * 2.0
    }
}

/// The cube as far as the end of phase C goes, for one labelling of the
/// wings: the centres by axis and the R centres among them, the positions of
/// the low wings, the flips of the middle edges, the parity of the wings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SState {
    /// U/D centres, R/L centres, R centres.
    pub x: [u32; 3],
    pub t: [u32; 3],
    pub low: u32,
    pub flip: u16,
    pub odd: bool,
}

pub struct Split {
    bx: BitPerm,
    bt: BitPerm,
    bw: BitPerm,
    flip_mv: Vec<[u16; 18]>,
}

impl Split {
    pub fn new() -> Split {
        let ct = cube_tables();
        Split {
            bx: BitPerm::from_src(&ct.ct_src),
            bt: BitPerm::from_src(&tables5().t_src),
            bw: BitPerm::from_src(&ct.ep_src),
            flip_mv: T3::new().flip_mv,
        }
    }

    /// What phase C sees of a cube with the wings labelled as they are.
    pub fn of(c: &Cube5) -> SState {
        let masks = |col: &[u8; 24]| {
            let mut m = [0u32; 3];
            for (i, &c) in col.iter().enumerate() {
                m[0] |= ((c % 3 == 0) as u32) << i;
                m[1] |= ((c % 3 == 1) as u32) << i;
                m[2] |= ((c == 1) as u32) << i;
            }
            m
        };
        SState {
            x: masks(&c.c4.ct),
            t: masks(&c.t),
            low: (0..24).fold(0, |m, p| m | ((c.c4.ep[p] < 12) as u32) << p),
            flip: (0..11).fold(0, |m, s| m | (c.mid.eo[s] as u16) << s),
            odd: c.c4.wing_parity() == 1,
        }
    }
}

impl Split {
    /// What phase C sees of a cube under a labelling of its wings: bit c of
    /// `lab` set makes the high wing of class c the low one.
    pub fn labelled(c: &Cube5, lab: u16) -> SState {
        let mut s = Split::of(c);
        s.low = (0..24).fold(0, |m, p| {
            let e = c.c4.ep[p];
            m | ((((e / 12) as u16 ^ (lab >> (e % 12))) & 1 == 0) as u32) << p
        });
        s.flip = (0..11).fold(0, |m, sl| m | ((c.mid.eo[sl] as u16 ^ (lab >> c.mid.ep[sl])) & 1) << sl);
        s
    }
}

impl Default for Split {
    fn default() -> Split {
        Split::new()
    }
}

impl Space for Split {
    type S = SState;

    #[inline(always)]
    fn step(&self, s: &SState, m: u8) -> SState {
        SState {
            x: s.x.map(|v| self.bx.apply(m, v)),
            t: s.t.map(|v| self.bt.apply(m, v)),
            low: self.bw.apply(m, s.low),
            // A wide turn carries the same middle edges as the face turn.
            flip: self.flip_mv[s.flip as usize][m as usize % 18],
            odd: s.odd ^ odd_move(m),
        }
    }

    #[inline(always)]
    fn key(&self, s: &SState) -> u64 {
        mix(&[
            s.x[0] as u64 | (s.x[1] as u64) << 24,
            s.x[2] as u64 | (s.t[0] as u64) << 24,
            s.t[1] as u64 | (s.t[2] as u64) << 24,
            s.low as u64 | (s.flip as u64) << 24 | (s.odd as u64) << 36,
        ])
    }

    /// Every R-against-L pattern the moves of the later phases reach from
    /// the solved one, as in `edges5`.
    fn goals(&self) -> Vec<SState> {
        let solved = (0x0F_0000u32, 0x0F_0000u32);
        let mut ready = vec![solved];
        let mut at = 0;
        while at < ready.len() {
            let (x, t) = ready[at];
            at += 1;
            for &m in &G_MOVES {
                let next = (self.bx.apply(m, x), self.bt.apply(m, t));
                if !ready.contains(&next) {
                    ready.push(next);
                }
            }
        }
        ready
            .into_iter()
            .map(|(x, t)| SState { x: [0x00_00FF, 0xFF_0000, x], t: [0x00_00FF, 0xFF_0000, t], low: 0x000FFF, flip: 0, odd: false })
            .collect()
    }

    fn size(&self) -> f64 {
        let orbit = 735_471.0 * 12_870.0 * 70.0; // 8 of 24, 8 of 16, 4 of 8
        orbit * orbit * 2_704_156.0 * 2048.0 * 2.0
    }
}

#[derive(Default)]
struct Plain(u64);

impl Hasher for Plain {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("keys are whole words");
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

type Seen = HashMap<u64, u8, BuildHasherDefault<Plain>>;

#[inline(always)]
fn shard(key: u64) -> usize {
    (key >> 40) as usize % SHARDS
}

/// The whole levels: their sizes, every cube of them by key, and the cubes
/// of the last one.
pub struct Whole<S> {
    pub sizes: Vec<u64>,
    seen: Vec<Seen>,
    last: Vec<S>,
}

impl<S> Whole<S> {
    /// How many moves from the goal a cube of the whole levels is.
    #[inline(always)]
    pub fn depth(&self, key: u64) -> Option<u8> {
        self.seen[shard(key)].get(&key).copied()
    }
}

/// Levels from the goal, each made whole, for as long as the next one is
/// expected to hold at most `cap` cubes.
pub fn whole<P: Space>(sp: &P, cap: usize) -> Whole<P::S> {
    let seen: Vec<Mutex<Seen>> = (0..SHARDS).map(|_| Mutex::new(Seen::default())).collect();
    let mut frontier = Vec::new();
    for g in sp.goals() {
        let k = sp.key(&g);
        if seen[shard(k)].lock().unwrap().insert(k, 0).is_none() {
            frontier.push(g);
        }
    }
    let mut sizes = vec![frontier.len() as u64];
    loop {
        let d = sizes.len();
        let growth = if d >= 3 { sizes[d - 1] as f64 / sizes[d - 2] as f64 } else { 36.0 };
        if frontier.len() as f64 * growth > cap as f64 {
            break;
        }
        let next: Vec<P::S> = frontier
            .par_chunks(1024)
            .flat_map_iter(|chunk| {
                let mut new = Vec::new();
                for s in chunk {
                    for m in 0..36u8 {
                        let c = sp.step(s, m);
                        let k = sp.key(&c);
                        if let Entry::Vacant(e) = seen[shard(k)].lock().unwrap().entry(k) {
                            e.insert(d as u8);
                            new.push(c);
                        }
                    }
                }
                new
            })
            .collect();
        if next.is_empty() {
            break;
        }
        sizes.push(next.len() as u64);
        frontier = next;
    }
    Whole { sizes, seen: seen.into_iter().map(|m| m.into_inner().unwrap()).collect(), last: frontier }
}

struct Probe<'a, P: Space> {
    sp: &'a P,
    seen: &'a [Seen],
    lookups: u64,
}

impl<P: Space> Probe<'_, P> {
    /// Whether some path of `len` moves, no move undoing or reordering the
    /// one before, leads from `s` to a cube of the whole levels.
    fn path(&mut self, s: &P::S, len: usize, last: u8) -> bool {
        if len == 0 {
            self.lookups += 1;
            let k = self.sp.key(s);
            return self.seen[shard(k)].contains_key(&k);
        }
        for m in 0..36u8 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            if self.path(&self.sp.step(s, m), len - 1, m) {
                return true;
            }
        }
        false
    }

    /// Whether `s` is at most `past` moves past the last whole level.
    fn within(&mut self, s: &P::S, past: usize) -> bool {
        (0..=past).any(|len| self.path(s, len, 255))
    }

    /// The different cubes one move from `s`.
    fn around(&self, s: &P::S) -> Vec<P::S> {
        let mut keys = Vec::with_capacity(36);
        let mut out = Vec::with_capacity(36);
        for m in 0..36u8 {
            let c = self.sp.step(s, m);
            let k = self.sp.key(&c);
            if !keys.contains(&k) {
                keys.push(k);
                out.push(c);
            }
        }
        out
    }

    /// One random descent of `deeper` levels from a cube of the last whole
    /// level: the weight after each step (zero from where it ends).
    fn descend(&mut self, start: &P::S, deeper: usize, rnd: &mut impl FnMut(usize) -> usize) -> Vec<f64> {
        let mut out = vec![0.0; deeper];
        let (mut cur, mut w) = (*start, 1.0);
        for j in 1..=deeper {
            let far: Vec<P::S> = self.around(&cur).into_iter().filter(|c| !self.within(c, j - 1)).collect();
            if far.is_empty() {
                break;
            }
            let c = far[rnd(far.len())];
            let parents = self.around(&c).into_iter().filter(|n| self.within(n, j - 1)).count();
            w *= far.len() as f64 / parents as f64;
            out[j - 1] = w;
            cur = c;
        }
        out
    }
}

/// The sizes of `deeper` levels past the whole ones, each with its standard
/// error, from `samples` descents; and the cubes looked up.
pub fn estimate<P: Space>(sp: &P, w: &Whole<P::S>, deeper: usize, samples: usize, seed: u64) -> (Vec<(f64, f64)>, u64) {
    let runs: Vec<(Vec<f64>, u64)> = (0..samples)
        .into_par_iter()
        .map(|i| {
            let mut state = seed << 32 ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
            let mut rnd = |n: usize| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (state >> 33) as usize % n
            };
            for _ in 0..4 {
                rnd(2);
            }
            let start = w.last[rnd(w.last.len())];
            let mut probe = Probe { sp, seen: &w.seen, lookups: 0 };
            (probe.descend(&start, deeper, &mut rnd), probe.lookups)
        })
        .collect();
    let (n, base) = (samples as f64, *w.sizes.last().unwrap() as f64);
    let levels = (0..deeper)
        .map(|j| {
            let mean = runs.iter().map(|r| r.0[j]).sum::<f64>() / n;
            let var = runs.iter().map(|r| (r.0[j] - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
            (base * mean, base * (var / n).sqrt())
        })
        .collect();
    (levels, runs.iter().map(|r| r.1).sum())
}

/// Move sequences of each length up to `max` in which no move undoes or
/// reorders the one before: what the count takes for different cubes.
pub fn sequences(max: usize) -> Vec<f64> {
    let mut by_last = [1.0f64; 36];
    let mut out = vec![1.0, 36.0];
    for _ in 2..=max {
        let mut next = [0.0f64; 36];
        for last in 0..36u8 {
            for m in 0..36u8 {
                if !skip_move(last, m) {
                    next[m as usize] += by_last[last as usize];
                }
            }
        }
        by_last = next;
        out.push(by_last.iter().sum());
    }
    out.truncate(max + 1);
    out
}

/// The share of random cubes within each number of moves of the goal, when
/// a cube has `targets` goals to be near and level d holds `sizes[d]` cubes
/// of the `size` there are; and the mean distance.
pub fn shares(sizes: &[f64], size: f64, targets: f64) -> (Vec<f64>, f64) {
    let (mut sum, mut mean) = (0.0, 0.0);
    let mut out = Vec::new();
    for &n in sizes {
        sum += n;
        let p = if sum >= size { 1.0 } else { -(targets * (-sum / size).ln_1p()).exp_m1() };
        out.push(p);
        mean += 1.0 - p;
    }
    (out, mean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_split_state_follows_the_cube() {
        let sp = Split::new();
        let mut cube = Cube5::solved();
        let mut s = Split::of(&cube);
        assert_eq!(sp.goals()[0], s);
        let mut seed = 11u64;
        for _ in 0..400 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let m = ((seed >> 33) % 36) as u8;
            cube.apply(m);
            s = sp.step(&s, m);
            assert_eq!(s, Split::of(&cube));
        }
    }

    #[test]
    fn the_first_levels_agree_with_the_descents() {
        // Whole levels to depth 4, and the last of them again from descents
        // that start a level earlier.
        let sp = Centres::new();
        let full = whole(&sp, 200_000);
        let short = whole(&sp, 200_000 / 40);
        assert!(short.sizes.len() < full.sizes.len());
        let (est, _) = estimate(&sp, &short, 1, 20_000, 3);
        let want = full.sizes[short.sizes.len()] as f64;
        assert!((est[0].0 - want).abs() < 5.0 * est[0].1 + 1.0, "{} against {}", est[0].0, want);
    }

    #[test]
    fn sequences_grow_as_counted() {
        let s = sequences(12);
        assert_eq!(s[1], 36.0);
        assert!((s[12] / s[11] - 28.12).abs() < 0.01);
    }
}
