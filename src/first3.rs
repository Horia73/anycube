//! First phase of the 3x3x3: the exact number of moves from every position
//! to the domino subgroup <U, D, R2, L2, F2, B2>, in a table small enough to
//! stay near the processor, and the shortest ways themselves for the
//! positions close to the subgroup.
//!
//! A position counts here by its twist, its flip and where its four slice
//! edges are: 2187 x 2048 x 495 of them. The sixteen symmetries of the cube
//! that keep the U/D axis take a position to one equally far, so flip and
//! slice together are put into 64,430 classes and the table has a row per
//! class, with the twist turned along: 141 million distances, modulo 3 in
//! two bits each (35 MB; `big1.rs` stores all 2.2 billion in 554 MB).
//!
//! For the positions within `NEAR` moves of the subgroup the file also has
//! - which moves lead one nearer and which keep the distance, and
//! - every shortest way into the subgroup, written out.
//!
//! For the eight million positions one move further out it has the moves
//! that lead nearer, and no more: a search that must arrive from one of
//! them in exactly that many moves reads those, where it would try every
//! move against the table of distances.
//!
//! A search that must arrive in exactly as many moves as a position is far
//! then reads its ways one after the other; walking them move by move costs
//! a table entry for every move tried at every step. The positions are
//! found through two lists: those nearer than `NEAR`, few enough (75
//! thousand) to stay in the processor's cache, and the ten times as many at
//! exactly `NEAR`.

use crate::c3::{N_FLIP, N_SLICE, N_TWIST, T3};
use crate::cube::skip_move;
use crate::sym::conjugators;
use crate::tables::{load_or_gen, table_dir, Bytes};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

const N_FS: usize = N_FLIP * N_SLICE;
const N_SYM: usize = 16;
const N_CLASS: usize = 64430;
const N_KEY: usize = N_CLASS * N_TWIST;
/// Positions this near the subgroup have their moves and their ways listed.
pub const NEAR: u8 = 7;
/// Sizes of the two lists, as powers of two: positions nearer than `NEAR`, positions at `NEAR`.
const SLOT_BITS: [u32; 2] = [17, 20];
/// Shortest ways of all the listed positions together.
const N_WAYS: usize = 2_509_109;
/// Positions `NEAR + 1` moves from the subgroup.
const N_NEXT: usize = 7_950_748;
/// They are found by counting: a bit for every position of the table, set for these, in
/// blocks of seven words behind the number of bits set before the block.
const BLOCK: usize = 7 * 64;
const EMPTY: u64 = u64::MAX;
const UNREACHED: u8 = 255;

// The file: class and symmetry of every flip and slice, the twist under each
// symmetry, the moves as each symmetry names them, the distances, the two
// lists of positions near the subgroup, and their ways.
const FS_CLASS: usize = 0;
const TW_CONJ: usize = FS_CLASS + 4 * N_FS;
const RAW_MOVE: usize = TW_CONJ + 2 * N_SYM * N_TWIST;
const EXACT: usize = RAW_MOVE + N_SYM * 18;
const SLOTS: [usize; 2] = [EXACT + N_KEY.div_ceil(32) * 8, EXACT + N_KEY.div_ceil(32) * 8 + (16 << SLOT_BITS[0])];
const WAYS: usize = SLOTS[1] + (16 << SLOT_BITS[1]);
const NEXT_BLOCKS: usize = WAYS + 8 * N_WAYS;
const NEXT_MOVES: usize = NEXT_BLOCKS + 64 * N_KEY.div_ceil(BLOCK);
const LEN: usize = NEXT_MOVES + 3 * N_NEXT;
const FILE: &str = "phase1-333.bin";

pub struct First3 {
    data: Bytes,
    /// Class and symmetry of every flip and slice, and the twist under each symmetry: the
    /// first two parts of the file, as numbers.
    fs_class: Vec<u32>,
    tw_conj: Vec<u16>,
    /// Per symmetry: the cube's own name of each move of the class's first member.
    raw_move: [[u8; 32]; N_SYM],
    /// The same for sets of moves, six bits at a time.
    raw_mask: Vec<[[u32; 64]; 3]>,
}

#[inline(always)]
fn slot_of(key: usize, bits: u32) -> usize {
    ((key as u32).wrapping_mul(0x9E37_79B1) >> (32 - bits)) as usize
}

/// The moves renamed by a symmetry: where each face goes (U R F D L B) and whether the turns change direction.
fn renaming(faces: [u8; 6], mirrored: bool) -> [u8; 18] {
    let mut out = [0u8; 18];
    for m in 0..18u8 {
        out[m as usize] = faces[(m / 3) as usize] * 3 + if mirrored { 2 - m % 3 } else { m % 3 };
    }
    out
}

/// Every shortest way from a position `d` moves away, five bits a move, the first move lowest.
fn shortest(
    step: &dyn Fn(usize, usize, usize) -> (usize, usize),
    dist: &dyn Fn(usize, usize) -> u8,
    at: (usize, usize),
    d: u8,
    last: u8,
    so_far: (u64, u32),
    out: &mut Vec<u64>,
) {
    if d == 0 {
        out.push(so_far.0);
        return;
    }
    for m in 0..18u8 {
        if last != u8::MAX && skip_move(last, m) {
            continue;
        }
        let next = step(m as usize, at.0, at.1);
        if dist(next.0, next.1) + 1 == d {
            shortest(step, dist, next, d - 1, m, (so_far.0 | (m as u64) << (5 * so_far.1), so_far.1 + 1), out);
        }
    }
}

fn generate(t: &T3) -> Vec<u8> {
    // A quarter turn of the cube about the U/D axis, a half turn about the R/L axis, the mirror image left to right.
    let generators =
        [renaming([0, 5, 1, 3, 2, 4], false), renaming([3, 1, 5, 0, 4, 2], false), renaming([0, 4, 2, 3, 1, 5], true)];
    let mut group: Vec<[u8; 18]> = vec![std::array::from_fn(|m| m as u8)];
    let mut i = 0;
    while i < group.len() {
        for g in &generators {
            let product: [u8; 18] = std::array::from_fn(|m| g[group[i][m] as usize]);
            if !group.contains(&product) {
                group.push(product);
            }
        }
        i += 1;
    }
    assert_eq!(group.len(), N_SYM, "symmetries that keep the U/D axis");
    let goal = t.slice_goal();
    let tw_act = |m: usize, x: usize| t.twist_mv[x][m] as usize;
    let fs_act =
        |m: usize, x: usize| t.flip_mv[x / N_SLICE][m] as usize * N_SLICE + t.slice_mv[x % N_SLICE][m] as usize;
    // Each symmetry as it acts on the twist, and on flip and slice together.
    let maps: Vec<(Vec<u32>, Vec<u32>)> = group
        .par_iter()
        .map(|mu| {
            let pairs: Vec<(usize, usize)> = (0..18).map(|m| (m, mu[m] as usize)).collect();
            let tw = conjugators(N_TWIST, &pairs, &tw_act, &tw_act, &[(0, 0)]);
            let fs = conjugators(N_FS, &pairs, &fs_act, &fs_act, &[(goal, goal)]);
            assert!(tw.len() == 1 && fs.len() == 1, "one map of the twist and one of flip and slice per symmetry");
            (tw[0].clone(), fs[0].clone())
        })
        .collect();
    // Classes of flip and slice: the smallest image names the class.
    let best: Vec<(u32, u8)> =
        (0..N_FS).into_par_iter().map(|x| (0..N_SYM).map(|s| (maps[s].1[x], s as u8)).min().unwrap()).collect();
    let mut reps: Vec<u32> = best.iter().map(|b| b.0).collect();
    reps.par_sort_unstable();
    reps.dedup();
    assert_eq!(reps.len(), N_CLASS, "classes of flip and slice");
    let class: Vec<u32> = best.par_iter().map(|b| reps.binary_search(&b.0).unwrap() as u32).collect();
    // The symmetries that keep a class's first member in place: its positions have that many names.
    let stab: Vec<u16> = reps
        .iter()
        .map(|&r| (0..N_SYM).fold(0u16, |a, s| a | ((maps[s].1[r as usize] == r) as u16) << s))
        .collect();
    let key_of = |tw: usize, fs: usize| class[fs] as usize * N_TWIST + maps[best[fs].1 as usize].0[tw] as usize;
    let step = |m: usize, tw: usize, fs: usize| (tw_act(m, tw), fs_act(m, fs));

    let cells: Vec<AtomicU8> = (0..N_KEY).into_par_iter().map(|_| AtomicU8::new(UNREACHED)).collect();
    let put = |key: usize, v: u8| -> u64 {
        let (c, tw) = (key / N_TWIST, key % N_TWIST);
        let mut new = 0;
        let mut st = stab[c];
        while st != 0 {
            let s = st.trailing_zeros() as usize;
            st &= st - 1;
            let cell = &cells[c * N_TWIST + maps[s].0[tw] as usize];
            new += cell.compare_exchange(UNREACHED, v, Relaxed, Relaxed).is_ok() as u64;
        }
        new
    };
    let mut total = put(key_of(0, goal), 0);
    let mut newest = total;
    for depth in 0u8.. {
        // Once fewer positions are left than were reached last, each of those looks for a
        // neighbour reached last; before that, each one reached last marks its neighbours.
        let backward = N_KEY as u64 - total < newest;
        let new: u64 = (0..N_CLASS)
            .into_par_iter()
            .map(|c| {
                let mut new = 0;
                let fs = reps[c] as usize;
                for tw in 0..N_TWIST {
                    let cell = &cells[c * N_TWIST + tw];
                    let at = cell.load(Relaxed);
                    if backward {
                        let next_to_last = |m: usize| {
                            let (tw2, fs2) = step(m, tw, fs);
                            cells[key_of(tw2, fs2)].load(Relaxed) == depth
                        };
                        if at == UNREACHED && (0..18).any(next_to_last) {
                            cell.store(depth + 1, Relaxed);
                            new += 1;
                        }
                    } else if at == depth {
                        for m in 0..18 {
                            let (tw2, fs2) = step(m, tw, fs);
                            new += put(key_of(tw2, fs2), depth + 1);
                        }
                    }
                }
                new
            })
            .sum();
        if new == 0 {
            break;
        }
        total += new;
        newest = new;
        eprintln!("  3x3x3 phase 1 table: depth {:2}  {:>10} new  {:>10} total", depth + 1, new, total);
    }
    assert_eq!(total as usize, N_KEY, "every position of the first phase is reached");
    let dist = |tw: usize, fs: usize| cells[key_of(tw, fs)].load(Relaxed);

    // The positions near the subgroup: their moves and their shortest ways, the nearest first.
    let mut near: Vec<(u8, usize, u64, Vec<u64>)> = (0..N_CLASS)
        .into_par_iter()
        .flat_map_iter(|c| {
            let mut found = Vec::new();
            for tw in 0..N_TWIST {
                let d = cells[c * N_TWIST + tw].load(Relaxed);
                if d > NEAR {
                    continue;
                }
                let fs = reps[c] as usize;
                let (mut down, mut flat) = (0u64, 0u64);
                for m in 0..18 {
                    let (tw2, fs2) = step(m, tw, fs);
                    let dc = dist(tw2, fs2);
                    down |= ((dc + 1 == d) as u64) << m;
                    flat |= ((dc == d) as u64) << m;
                }
                let mut ways = Vec::new();
                shortest(&step, &dist, (tw, fs), d, u8::MAX, (0, 0), &mut ways);
                found.push((d, c * N_TWIST + tw, down << 18 | flat, ways));
            }
            found
        })
        .collect();
    near.par_sort_unstable_by_key(|n| (n.0, n.1));
    let mut slots: [Vec<[u64; 2]>; 2] = SLOT_BITS.map(|bits| vec![[EMPTY; 2]; 1 << bits]);
    let mut ways = Vec::new();
    let mut listed = [0usize; 2];
    for (d, key, moves, found) in &near {
        assert!(found.len() < 1 << 16, "a position has fewer than 65536 shortest ways");
        let list = (*d == NEAR) as usize;
        let mut at = slot_of(*key, SLOT_BITS[list]);
        while slots[list][at][0] != EMPTY {
            at = (at + 1) & ((1 << SLOT_BITS[list]) - 1);
        }
        slots[list][at] = [(*key as u64) << 36 | moves, (ways.len() as u64) << 16 | found.len() as u64];
        ways.extend_from_slice(found);
        listed[list] += 1;
    }
    eprintln!("  3x3x3 phase 1 table: {} and {} positions near the subgroup, {} ways", listed[0], listed[1], ways.len());
    for list in 0..2 {
        assert!(listed[list] * 5 < 4 << SLOT_BITS[list], "a list of near positions is at most four fifths full");
    }
    assert_eq!(ways.len(), N_WAYS, "shortest ways of the positions near the subgroup");

    let mut out = Vec::with_capacity(LEN);
    out.extend((0..N_FS).flat_map(|x| (class[x] << 4 | best[x].1 as u32).to_le_bytes()));
    out.extend((0..N_TWIST).flat_map(|tw| (0..N_SYM).map(move |s| (tw, s))).flat_map(|(tw, s)| (maps[s].0[tw] as u16).to_le_bytes()));
    for mu in &group {
        out.extend((0..18u8).map(|named| mu.iter().position(|&x| x == named).unwrap() as u8));
    }
    assert_eq!(out.len(), EXACT);
    out.resize(SLOTS[0], 0);
    out[EXACT..].par_iter_mut().enumerate().for_each(|(i, byte)| {
        for key in (4 * i..4 * i + 4).filter(|&key| key < N_KEY) {
            *byte |= (cells[key].load(Relaxed) % 3) << ((key & 3) * 2);
        }
    });
    out.extend(slots.iter().flatten().flatten().flat_map(|x| x.to_le_bytes()));
    out.extend(ways.iter().flat_map(|x| x.to_le_bytes()));

    // The positions one move further out, in the order of the table, and their moves that lead nearer.
    let next: Vec<usize> = (0..N_KEY).into_par_iter().filter(|&key| cells[key].load(Relaxed) == NEAR + 1).collect();
    assert_eq!(next.len(), N_NEXT, "positions one move further than the listed ones");
    let mut blocks = vec![0u64; 8 * N_KEY.div_ceil(BLOCK)];
    for &key in &next {
        let (block, bit) = (key / BLOCK, key % BLOCK);
        blocks[8 * block + 1 + bit / 64] |= 1 << (bit % 64);
    }
    let mut before = 0;
    for block in blocks.chunks_exact_mut(8) {
        block[0] = before;
        before += block[1..].iter().map(|w| w.count_ones() as u64).sum::<u64>();
    }
    out.extend(blocks.iter().flat_map(|x| x.to_le_bytes()));
    let nearer: Vec<[u8; 3]> = next
        .par_iter()
        .map(|&key| {
            let (c, tw) = (key / N_TWIST, key % N_TWIST);
            let down = (0..18).fold(0u32, |a, m| {
                let (tw2, fs2) = step(m, tw, reps[c] as usize);
                a | ((dist(tw2, fs2) == NEAR) as u32) << m
            });
            [down as u8, (down >> 8) as u8, (down >> 16) as u8]
        })
        .collect();
    out.extend(nearer.iter().flatten());
    out
}

impl First3 {
    pub fn load(t: &T3) -> First3 {
        let data = load_or_gen(&table_dir().join(FILE), LEN, || generate(t));
        let fs_class = data[FS_CLASS..TW_CONJ].chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect();
        let tw_conj = data[TW_CONJ..RAW_MOVE].chunks_exact(2).map(|b| u16::from_le_bytes(b.try_into().unwrap())).collect();
        let mut raw_move = [[0u8; 32]; N_SYM];
        let mut raw_mask = vec![[[0u32; 64]; 3]; N_SYM];
        for s in 0..N_SYM {
            raw_move[s][..18].copy_from_slice(&data[RAW_MOVE + s * 18..RAW_MOVE + s * 18 + 18]);
            for part in 0..3 {
                for bits in 0..64usize {
                    for j in (0..6).filter(|j| bits >> j & 1 == 1) {
                        raw_mask[s][part][bits] |= 1 << raw_move[s][part * 6 + j];
                    }
                }
            }
        }
        First3 { data, fs_class, tw_conj, raw_move, raw_mask }
    }

    #[inline(always)]
    fn word(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.data[at..at + 8].try_into().unwrap())
    }

    /// Where a position is in the table, and the symmetry that takes it to its class's first member.
    #[inline(always)]
    pub fn key(&self, tw: usize, fl: usize, sl: usize) -> (usize, usize) {
        let e = self.fs_class[fl * N_SLICE + sl] as usize;
        let s = e & 15;
        ((e >> 4) * N_TWIST + self.tw_conj[tw * N_SYM + s] as usize, s)
    }

    /// Distance modulo 3.
    #[inline(always)]
    pub fn get(&self, key: usize) -> u8 {
        (self.data[EXACT + (key >> 2)] >> ((key & 3) * 2)) & 3
    }

    /// The moves of a position `dist` moves from the subgroup, `NEAR` at most, that lead one
    /// nearer and those that keep the distance, as bit masks of the cube's own moves.
    #[inline(always)]
    pub fn near(&self, key: usize, sym: usize, dist: u8) -> (u32, u32) {
        let list = (dist == NEAR) as usize;
        let (bits, slots) = (SLOT_BITS[list], SLOTS[list]);
        let mut at = slot_of(key, bits);
        loop {
            let e = self.word(slots + 16 * at);
            if (e >> 36) as usize == key {
                let t = &self.raw_mask[sym];
                let named = |x: u64| t[0][(x & 63) as usize] | t[1][(x >> 6 & 63) as usize] | t[2][(x >> 12 & 63) as usize];
                return (named(e >> 18), named(e));
            }
            assert!(e != EMPTY, "a position near the subgroup is listed");
            at = (at + 1) & ((1 << bits) - 1);
        }
    }

    /// The moves that lead nearer from a position `NEAR + 1` moves from the subgroup, as a bit
    /// mask of the cube's own moves.
    #[inline(always)]
    pub fn nearer_from_next(&self, key: usize, sym: usize) -> u32 {
        let (block, bit) = (NEXT_BLOCKS + 64 * (key / BLOCK), key % BLOCK);
        let mut rank = self.word(block) as usize;
        for w in 0..bit / 64 {
            rank += self.word(block + 8 + 8 * w).count_ones() as usize;
        }
        rank += (self.word(block + 8 + 8 * (bit / 64)) & ((1 << (bit % 64)) - 1)).count_ones() as usize;
        let d = &self.data[NEXT_MOVES + 3 * rank..NEXT_MOVES + 3 * rank + 3];
        let down = d[0] as usize | (d[1] as usize) << 8 | (d[2] as usize) << 16;
        let t = &self.raw_mask[sym];
        t[0][down & 63] | t[1][down >> 6 & 63] | t[2][down >> 12]
    }

    /// The first place to look for a listed position: reading it brings that part of the list
    /// into the processor's cache.
    #[inline(always)]
    pub fn peek(&self, key: usize, dist: u8) -> u64 {
        let list = (dist == NEAR) as usize;
        self.word(SLOTS[list] + 16 * slot_of(key, SLOT_BITS[list]))
    }

    /// Where the shortest ways of a listed position start, and how many there are.
    #[inline(always)]
    pub fn ways(&self, key: usize, dist: u8) -> (u32, u32) {
        let list = (dist == NEAR) as usize;
        let (bits, slots) = (SLOT_BITS[list], SLOTS[list]);
        let mut at = slot_of(key, bits);
        loop {
            let e = self.word(slots + 16 * at);
            if (e >> 36) as usize == key {
                let ways = self.word(slots + 16 * at + 8);
                return ((ways >> 16) as u32, (ways & 0xFFFF) as u32);
            }
            assert!(e != EMPTY, "a position near the subgroup is listed");
            at = (at + 1) & ((1 << bits) - 1);
        }
    }

    /// One of the shortest ways of a listed position: five bits a move, the
    /// first move lowest, in the names of its class's first member.
    #[inline(always)]
    pub fn way(&self, i: usize) -> u64 {
        self.word(WAYS + 8 * i)
    }

    /// The cube's own names of the moves of a class's first member, under the symmetry `key` gave.
    /// (Room for every five bits of a way, so that a move is read without a check.)
    #[inline(always)]
    pub fn raw_moves(&self, sym: usize) -> &[u8; 32] {
        &self.raw_move[sym]
    }

    /// Exact distance, found by walking down to the subgroup.
    pub fn exact(&self, t: &T3, mut tw: usize, mut fl: usize, mut sl: usize) -> u8 {
        let goal = self.key(0, 0, t.slice_goal()).0;
        let mut d = 0u8;
        let mut cur = self.get(self.key(tw, fl, sl).0);
        while self.key(tw, fl, sl).0 != goal {
            let want = (cur + 2) % 3;
            let step = (0..18).find_map(|m| {
                let (a, b, c) = (t.twist_mv[tw][m] as usize, t.flip_mv[fl][m] as usize, t.slice_mv[sl][m] as usize);
                (self.get(self.key(a, b, c).0) == want).then_some((a, b, c))
            });
            (tw, fl, sl) = step.expect("some move leads nearer");
            cur = want;
            d += 1;
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero only in the subgroup, never more than one apart across a move,
    /// and from every position some move leads one nearer: together these
    /// make the distances exact. The moves listed for the near positions
    /// are the ones the distances give, and each of their ways arrives.
    #[test]
    fn distances_are_exact_and_near_positions_agree() {
        let t = T3::new();
        let f = First3::load(&t);
        let goal = t.slice_goal();
        assert_eq!(f.exact(&t, 0, 0, goal), 0);
        let mut seed = 11u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        let (mut near, mut further) = (0, 0);
        for round in 0..3_000 {
            let (mut tw, mut fl, mut sl) = (0, 0, goal);
            for _ in 0..if round % 2 == 0 { rnd(10) } else { 30 } {
                let m = rnd(18);
                (tw, fl, sl) = (t.twist_mv[tw][m] as usize, t.flip_mv[fl][m] as usize, t.slice_mv[sl][m] as usize);
            }
            let d = f.exact(&t, tw, fl, sl);
            assert_eq!(d == 0, tw == 0 && fl == 0 && sl == goal);
            let next: Vec<u8> = (0..18)
                .map(|m| f.exact(&t, t.twist_mv[tw][m] as usize, t.flip_mv[fl][m] as usize, t.slice_mv[sl][m] as usize))
                .collect();
            assert!(next.iter().all(|&n| n + 1 >= d && n <= d + 1));
            assert!(d == 0 || next.iter().any(|&n| n + 1 == d));
            if d == NEAR + 1 {
                let (key, sym) = f.key(tw, fl, sl);
                let down = f.nearer_from_next(key, sym);
                for m in 0..18 {
                    assert_eq!(down >> m & 1 == 1, next[m] + 1 == d, "move {} at distance {}", m, d);
                }
                further += 1;
            }
            if d <= NEAR {
                near += 1;
                let (key, sym) = f.key(tw, fl, sl);
                let (down, flat) = f.near(key, sym, d);
                for m in 0..18 {
                    assert_eq!(down >> m & 1 == 1, next[m] + 1 == d, "move {} at distance {}", m, d);
                    assert_eq!(flat >> m & 1 == 1, next[m] == d, "move {} at distance {}", m, d);
                }
                let (ways, count) = f.ways(key, d);
                assert!(count >= 1);
                let mut firsts = 0u32;
                for i in 0..count {
                    let mut way = f.way((ways + i) as usize);
                    firsts |= 1 << f.raw_moves(sym)[(way & 31) as usize];
                    let (mut a, mut b, mut c) = (tw, fl, sl);
                    for _ in 0..d {
                        let m = f.raw_moves(sym)[(way & 31) as usize] as usize;
                        (a, b, c) = (t.twist_mv[a][m] as usize, t.flip_mv[b][m] as usize, t.slice_mv[c][m] as usize);
                        way >>= 5;
                    }
                    assert!(way == 0 && a == 0 && b == 0 && c == goal, "a way of {} moves arrives", d);
                }
                // A first move of a way leads nearer; of two that may be done in either order one comes first.
                assert!(d == 0 || firsts & !down == 0 && firsts != 0);
            }
        }
        assert!(near > 1_000 && further > 50);
    }

    /// The same distances as the table without symmetry.
    #[test]
    #[ignore = "needs the 554 MB table of the first phase"]
    fn agrees_with_the_full_table() {
        let t = T3::new();
        let f = First3::load(&t);
        let big = crate::big1::Big1::load(&t);
        let mut seed = 5u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        for _ in 0..2_000_000 {
            let (tw, fl, sl) = (rnd(N_TWIST), rnd(N_FLIP), rnd(N_SLICE));
            assert_eq!(f.get(f.key(tw, fl, sl).0), big.get(crate::big1::Big1::index(tw, fl, sl)));
        }
    }
}
