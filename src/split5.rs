//! Experiment: a 5x5x5 cube brought to the end of phase C by a beam search
//! with all 36 moves, in place of the centre phases and phase C. Not part
//! of any mode.
//!
//! The beam holds cubes, not cubes under a labelling of the wings: a first
//! version started from every labelling and failed, because at the start
//! all 2,048 look alike and the beam fills with the wrong ones. Level by
//! level every cube kept is moved in every way, the same cube kept once,
//! and the `width` that the estimate puts nearest are kept. A cube whose
//! centres are near staged is looked up, under the few labellings its
//! middle edges allow, in the whole levels of `levels5` and finished
//! exactly; what the exact centre distances say cannot end sooner than the
//! shortest way found is dropped. The cube is searched in three frames,
//! one for each axis.
//!
//! The estimate is a table: for cubes K random moves from the goal, the mean
//! K by the exact moves that stage each centre orbit alone, the classes of
//! wings with both wings in one half, and the classes whose low wing does
//! not agree with the flip of their middle edge.
//!
//! Or a network (`Net`, trained by `ml/split5_net.py`) that reads the whole
//! cube: the kind of every centre, every wing and middle edge, the parity
//! of the wings. It sees about four moves farther than the table, and a
//! random cube is still out of reach of the search. `starts` hands the
//! cubes the network likes after a few moves to the solver; that loses to
//! the solver alone. Numbers in `bench/RESULTS.md`.

use crate::c5::{tables5, Cube5};
use crate::centres5::Centres5;
use crate::cube::skip_move;
use crate::edges5::{Edges5, G_MOVES};
use crate::levels5::{mix, whole, SState, Space, Split, Whole};
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;

const CELLS: usize = 16 * 16 * 13 * 13;

/// The inputs of the network: the kind of every x-centre and t-centre (U/D,
/// R, L, F/B), the wing in every wing position, the middle edge and its flip
/// in every slot, the parity of the wings. `N_ON` of them are on.
pub const N_IN: usize = 96 + 96 + 576 + 288 + 2;
pub const N_ON: usize = 24 + 24 + 24 + 12 + 1;
/// Bytes of a training record: the inputs that are on, the moves to the
/// goal, and whether those are a lower bound.
pub const RECORD: usize = 2 * N_ON + 6;

pub fn inputs(c: &Cube5) -> [u16; N_ON] {
    let kind = |col: u8| -> u16 {
        match col {
            0 | 3 => 0,
            1 => 1,
            4 => 2,
            _ => 3,
        }
    };
    let mut on = [0u16; N_ON];
    for p in 0..24 {
        on[p] = (4 * p) as u16 + kind(c.c4.ct[p]);
        on[24 + p] = 96 + (4 * p) as u16 + kind(c.t[p]);
        on[48 + p] = 192 + (24 * p) as u16 + c.c4.ep[p] as u16;
    }
    for sl in 0..12 {
        on[72 + sl] = 768 + (24 * sl) as u16 + 2 * c.mid.ep[sl] as u16 + c.mid.eo[sl] as u16;
    }
    on[84] = 1056 + c.c4.wing_parity() as u16;
    on
}

pub fn record(on: &[u16; N_ON], moves: f32, bound: bool) -> [u8; RECORD] {
    let mut out = [0u8; RECORD];
    for (i, v) in on.iter().enumerate() {
        out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
    }
    out[2 * N_ON..2 * N_ON + 4].copy_from_slice(&moves.to_le_bytes());
    out[2 * N_ON + 4] = bound as u8;
    out
}

/// A network of three hidden layers, as `ml/split5_net.py` writes it.
pub struct Net {
    sizes: [usize; 3],
    w1: Vec<f32>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: Vec<f32>,
    w3: Vec<f32>,
    b3: Vec<f32>,
    w4: Vec<f32>,
    b4: f32,
}

impl Net {
    pub fn load(path: &str) -> Result<Net, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path, e))?;
        if bytes.len() < 16 || &bytes[..4] != b"S5N1" {
            return Err(format!("{}: not a network of split5", path));
        }
        let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        let sizes = [word(4), word(8), word(12)];
        let mut at = 16;
        let mut take = |n: usize| -> Result<Vec<f32>, String> {
            let end = at + 4 * n;
            if end > bytes.len() {
                return Err(format!("{}: too short", path));
            }
            let v = bytes[at..end].chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
            at = end;
            Ok(v)
        };
        let [a, b, c] = sizes;
        Ok(Net {
            sizes,
            w1: take(N_IN * a)?,
            b1: take(a)?,
            w2: take(a * b)?,
            b2: take(b)?,
            w3: take(b * c)?,
            b3: take(c)?,
            w4: take(c)?,
            b4: take(1)?[0],
        })
    }

    /// The moves to the end of phase C the network gives a cube.
    pub fn value(&self, on: &[u16; N_ON]) -> f32 {
        let [a, b, c] = self.sizes;
        let mut h1 = self.b1.clone();
        for &i in on {
            for (h, w) in h1.iter_mut().zip(&self.w1[i as usize * a..][..a]) {
                *h += w;
            }
        }
        // The weights lie by input, so a unit that is off costs nothing.
        let layer = |x: &[f32], w: &[f32], bias: &[f32], n: usize| -> Vec<f32> {
            let mut y = bias.to_vec();
            for (j, &v) in x.iter().enumerate() {
                if v > 0.0 {
                    for (o, w) in y.iter_mut().zip(&w[j * n..][..n]) {
                        *o += v * w;
                    }
                }
            }
            y
        };
        let h2 = layer(&h1, &self.w2, &self.b2, b);
        let h3 = layer(&h2, &self.w3, &self.b3, c);
        self.b4 + h3.iter().zip(&self.w4).filter(|(v, _)| **v > 0.0).map(|(v, w)| v * w).sum::<f32>()
    }
}

/// A cube the search kept, as the network reads it, and its level.
pub struct Kept {
    pub on: [u16; N_ON],
    pub level: u8,
}

pub struct Tools {
    pub sp: Split,
    c5: Centres5,
    pub near: Whole<SState>,
    /// The flips of the middle edges (12 bits) a cube within the whole
    /// levels can have under its labelling.
    flips: Vec<u16>,
    /// The cells of the estimate that cubes within the whole levels fall in.
    near_cells: Vec<bool>,
    est: Vec<f32>,
    /// The estimate, in place of the table, if there is one.
    pub net: Option<Net>,
}

pub struct Way {
    pub frame: usize,
    pub moves: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Child {
    est: u32,
    key: u64,
    parent: u32,
    mv: u8,
}

/// What the estimate and the search read off a cube.
struct Seen {
    s: SState,
    cell: usize,
    /// The moves the centres need at least.
    least: u8,
}

pub fn lcg(seed: u64) -> impl FnMut(usize) -> usize {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    move |n| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) as usize % n
    }
}

/// The cube in the frame of each axis.
pub fn frames(facelets: &[u8]) -> Result<Vec<Cube5>, String> {
    let g = &tables5().geometry;
    [vec![], vec![0, 1], vec![0, 1, 0, 1]].iter().map(|turns| Cube5::from_facelets(&g.reframe(facelets, turns))).collect()
}

impl Tools {
    /// `cap`: the whole levels are made while the next is expected to hold
    /// at most that many cubes. `walks` cubes of 1 to `kmax` moves fill the
    /// estimate.
    pub fn load(cap: usize, walks: usize, kmax: usize) -> Tools {
        let sp = Split::new();
        let near = whole(&sp, cap);
        let deep = near.sizes.len() - 1;
        let flip_mv = &Edges5::load().t3.flip_mv;
        let mut flips = vec![0u16];
        let mut at = 0;
        for _ in 0..deep {
            let end = flips.len();
            for i in at..end {
                for m in 0..18 {
                    let f = flip_mv[flips[i] as usize][m];
                    if !flips.contains(&f) {
                        flips.push(f);
                    }
                }
            }
            at = end;
        }
        // The twelfth flip is the parity of the other eleven.
        let flips = flips.into_iter().map(|f| f | ((f.count_ones() as u16 & 1) << 11)).collect();
        let mut tools = Tools { sp, c5: Centres5::load(), near, flips, near_cells: vec![false; CELLS], est: vec![0.0; CELLS], net: None };
        // Every way of at most `deep` moves from a few goal cubes: only a cube
        // in one of their cells is looked up in the whole levels.
        let cells: Vec<usize> = (0..16u64)
            .into_par_iter()
            .flat_map_iter(|i| {
                let (cube, _) = walk(0, &mut lcg(1000 + i));
                let mut out = Vec::new();
                tools.cells_around(&cube, deep, 255, &mut out);
                out
            })
            .collect();
        for c in cells {
            tools.near_cells[c] = true;
        }
        let sums = (0..walks)
            .into_par_iter()
            .fold(
                || vec![(0.0f64, 0u32); CELLS],
                |mut acc, i| {
                    let mut rnd = lcg(i as u64 + 1);
                    let k = 1 + i % kmax;
                    let (cube, _) = walk(k, &mut rnd);
                    let cell = &mut acc[tools.see(&cube).cell];
                    cell.0 += k as f64;
                    cell.1 += 1;
                    acc
                },
            )
            .reduce(|| vec![(0.0, 0); CELLS], |mut a, b| {
                for (x, y) in a.iter_mut().zip(b) {
                    x.0 += y.0;
                    x.1 += y.1;
                }
                a
            });
        eprintln!("{} flips, {} cells near the goal", tools.flips.len(), tools.near_cells.iter().filter(|&&b| b).count());
        tools.est = sums.iter().map(|&(sum, n)| if n > 0 { (sum / n as f64) as f32 } else { kmax as f32 }).collect();
        tools
    }

    fn cells_around(&self, c: &Cube5, left: usize, last: u8, out: &mut Vec<usize>) {
        out.push(self.see(c).cell);
        if left == 0 {
            return;
        }
        for m in 0..36u8 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let mut next = *c;
            next.apply(m);
            self.cells_around(&next, left - 1, m, out);
        }
    }

    #[inline(always)]
    fn see(&self, c: &Cube5) -> Seen {
        let s = Split::of(c);
        let (hx, ht) = self.c5.bounds_of(s.x[1], s.x[0], s.t[1], s.t[0]);
        let mut at = [0u8; 24];
        for p in 0..24 {
            at[c.c4.ep[p] as usize] = p as u8;
        }
        let mut flip = [0u8; 12];
        for sl in 0..12 {
            flip[c.mid.ep[sl] as usize] = c.mid.eo[sl];
        }
        let (mut whole_classes, mut wrong) = (0u8, 0u8);
        for class in 0..12 {
            let (a, b) = (at[class] < 12, at[class + 12] < 12);
            if a == b {
                whole_classes += 1;
            } else if (b as u8) != flip[class] {
                // The wing in the low half is the one the middle edge's flip names.
                wrong += 1;
            }
        }
        let cell = (((hx as usize & 15) * 16 + (ht as usize & 15)) * 13 + whole_classes as usize) * 13 + wrong as usize;
        Seen { s, cell, least: hx.max(ht) }
    }

    #[inline(always)]
    fn score(&self, c: &Cube5, seen: &Seen) -> f32 {
        match &self.net {
            Some(net) => net.value(&inputs(c)).max(0.0),
            None => self.est[seen.cell],
        }
    }

    /// The estimate of the moves a cube is from the end of phase C.
    pub fn estimate(&self, c: &Cube5) -> f32 {
        self.score(c, &self.see(c))
    }

    /// 64 bits for the cube as far as the end of phase C goes: the centres
    /// by axis, every wing and middle edge.
    #[inline(always)]
    fn key(&self, c: &Cube5, s: &SState) -> u64 {
        let pack = |from: usize| (from..from + 12).fold(0u64, |w, p| w << 5 | c.c4.ep[p] as u64);
        let mid = (0..12).fold(0u64, |w, sl| w << 5 | (c.mid.ep[sl] as u64) << 1 | c.mid.eo[sl] as u64);
        mix(&[
            s.x[0] as u64 | (s.x[1] as u64) << 24,
            s.x[2] as u64 | (s.t[0] as u64) << 24,
            s.t[1] as u64 | (s.t[2] as u64) << 24 | (s.odd as u64) << 48,
            pack(0),
            pack(12),
            mid,
        ])
    }

    /// The moves a cube is from the end of phase C if it is within the whole
    /// levels, and the labelling under which it is.
    fn end(&self, c: &Cube5, seen: &Seen) -> Option<(u8, u16)> {
        let deep = self.near.sizes.len() - 1;
        if seen.least as usize > deep || !self.near_cells[seen.cell] {
            return None;
        }
        // Under the labelling that leaves no middle edge flipped, then with
        // the classes of the flipped ones changed over.
        let lab0 = (0..12).fold(0u16, |l, sl| l | (c.mid.eo[sl] as u16) << c.mid.ep[sl]);
        let base = Split::labelled(c, lab0);
        let mut swap = [0u32; 12];
        for p in 0..24 {
            let class = (c.c4.ep[p] % 12) as usize;
            let slot = (0..12).find(|&sl| c.mid.ep[sl] as usize == class).unwrap();
            swap[slot] |= 1 << p;
        }
        let mut best: Option<(u8, u16)> = None;
        for &f in &self.flips {
            let mut s = base;
            s.flip = f & 0x7FF;
            let mut lab = lab0;
            for sl in 0..12 {
                if (f >> sl) & 1 == 1 {
                    s.low ^= swap[sl];
                    lab ^= 1 << c.mid.ep[sl];
                }
            }
            if let Some(j) = self.near.depth(self.sp.key(&s)) {
                if best.map_or(true, |b| j < b.0) {
                    best = Some((j, lab));
                }
            }
        }
        best
    }

    /// Whether the cube is as phase C leaves it.
    pub fn ended(&self, c: &Cube5) -> bool {
        let lab = (0..12).fold(0u16, |l, sl| l | (c.mid.eo[sl] as u16) << c.mid.ep[sl]);
        self.near.depth(self.sp.key(&Split::labelled(c, lab))) == Some(0)
    }

    /// The `keep` cubes the estimate puts nearest after `depth` moves of a
    /// beam of `width`, for a solver to go on from: (frame, moves).
    pub fn starts(&self, roots: &[Cube5], width: usize, depth: usize, keep: usize) -> Vec<(usize, Vec<u8>)> {
        let mut cubes: Vec<Cube5> = roots.to_vec();
        let mut levels: Vec<Vec<(u32, u8)>> = vec![(0..roots.len() as u32).map(|i| (i, 255)).collect()];
        for d in 0..depth {
            let last = &levels[d];
            let mut kids: Vec<Child> = cubes
                .par_iter()
                .enumerate()
                .flat_map_iter(|(i, cube)| {
                    let mut out = Vec::with_capacity(30);
                    for m in 0..36u8 {
                        if last[i].1 != 255 && skip_move(last[i].1, m) {
                            continue;
                        }
                        let mut c = *cube;
                        c.apply(m);
                        let seen = self.see(&c);
                        out.push(Child { est: (self.score(&c, &seen) * 1024.0) as u32, key: self.key(&c, &seen.s), parent: i as u32, mv: m });
                    }
                    out
                })
                .collect();
            kids.par_sort_unstable_by_key(|c| c.key);
            kids.dedup_by_key(|c| c.key);
            let most = if d + 1 == depth { keep } else { width };
            if kids.len() > most {
                kids.select_nth_unstable_by_key(most, |c| c.est);
                kids.truncate(most);
            }
            cubes = kids
                .par_iter()
                .map(|k| {
                    let mut c = cubes[k.parent as usize];
                    c.apply(k.mv);
                    c
                })
                .collect();
            levels.push(kids.iter().map(|c| (c.parent, c.mv)).collect());
        }
        (0..cubes.len())
            .map(|i| {
                let (mut at, mut moves) = (i, Vec::new());
                for l in (1..=depth).rev() {
                    moves.push(levels[l][at].1);
                    at = levels[l][at].0 as usize;
                }
                moves.reverse();
                (at, moves)
            })
            .collect()
    }

    /// The shortest way to the end of phase C that a beam of `width` finds
    /// from the cube in any of its frames, among ways of at most `max_len`
    /// moves; and the cubes it scored.
    pub fn beam(&self, roots: &[Cube5], width: usize, max_len: usize) -> (Option<Way>, u64) {
        let (way, scored, _) = self.search(roots, width, max_len, 0);
        (way, scored)
    }

    /// As `beam`, and `take` of the cubes kept at every level: half those
    /// the estimate puts nearest, half any.
    pub fn search(&self, roots: &[Cube5], width: usize, max_len: usize, take: usize) -> (Option<Way>, u64, Vec<Kept>) {
        let mut kept: Vec<Kept> = Vec::new();
        let deep = self.near.sizes.len() - 1;
        let best = AtomicUsize::new(max_len + 1);
        // The level, the cube of it, the move after it (255: none).
        let lead: Mutex<Option<(usize, u32, u8)>> = Mutex::new(None);
        let offer = |len: usize, at: (usize, u32, u8)| {
            if len < best.load(Relaxed) {
                let mut l = lead.lock().unwrap();
                if len < best.load(Relaxed) {
                    best.store(len, Relaxed);
                    *l = Some(at);
                }
            }
        };
        let mut cubes: Vec<Cube5> = roots.to_vec();
        let mut levels: Vec<Vec<(u32, u8)>> = vec![(0..roots.len() as u32).map(|i| (i, 255)).collect()];
        for (i, c) in cubes.iter().enumerate() {
            if let Some((j, _)) = self.end(c, &self.see(c)) {
                offer(j as usize, (0, i as u32, 255));
            }
        }
        let mut scored = 0u64;
        let mut d = 0;
        // A cube kept is not in the whole levels: a way through it has more than `deep` moves left.
        while !cubes.is_empty() && d + deep + 1 < best.load(Relaxed) {
            let last = &levels[d];
            let mut kids: Vec<Child> = cubes
                .par_iter()
                .enumerate()
                .flat_map_iter(|(i, cube)| {
                    let mut out = Vec::with_capacity(30);
                    for m in 0..36u8 {
                        if last[i].1 != 255 && skip_move(last[i].1, m) {
                            continue;
                        }
                        let mut c = *cube;
                        c.apply(m);
                        let seen = self.see(&c);
                        if let Some((j, _)) = self.end(&c, &seen) {
                            offer(d + 1 + j as usize, (d, i as u32, m));
                            continue;
                        }
                        if d + 1 + (seen.least as usize).max(deep + 1) >= best.load(Relaxed) {
                            continue;
                        }
                        out.push(Child { est: (self.score(&c, &seen) * 1024.0) as u32, key: self.key(&c, &seen.s), parent: i as u32, mv: m });
                    }
                    out
                })
                .collect();
            scored += kids.len() as u64;
            kids.par_sort_unstable_by_key(|c| c.key);
            kids.dedup_by_key(|c| c.key);
            if kids.len() > width {
                kids.select_nth_unstable_by_key(width, |c| c.est);
                kids.truncate(width);
            }
            cubes = kids
                .par_iter()
                .map(|k| {
                    let mut c = cubes[k.parent as usize];
                    c.apply(k.mv);
                    c
                })
                .collect();
            levels.push(kids.iter().map(|c| (c.parent, c.mv)).collect());
            d += 1;
            if take > 0 && !kids.is_empty() {
                let mut order: Vec<(u32, u32)> = kids.iter().enumerate().map(|(i, k)| (k.est, i as u32)).collect();
                let near = (take / 2).min(order.len());
                if near < order.len() {
                    order.select_nth_unstable(near);
                }
                let mut rnd = lcg(scored);
                let pick = order[..near].iter().map(|&(_, i)| i as usize).chain((near..take).map(|_| rnd(kids.len())));
                kept.extend(pick.map(|i| Kept { on: inputs(&cubes[i]), level: d as u8 }).collect::<Vec<_>>());
            }
        }
        let Some((level, node, mv)) = *lead.lock().unwrap() else { return (None, scored, kept) };
        let mut moves = Vec::new();
        let mut at = node as usize;
        for l in (1..=level).rev() {
            moves.push(levels[l][at].1);
            at = levels[l][at].0 as usize;
        }
        moves.reverse();
        if mv != 255 {
            moves.push(mv);
        }
        let mut cube = roots[at];
        cube.apply_seq(&moves);
        // Down the whole levels to the goal, under the labelling found.
        let (left, lab) = self.end(&cube, &self.see(&cube)).expect("the cube the search ended on");
        let mut s = Split::labelled(&cube, lab);
        for j in (0..left).rev() {
            let m = (0..36u8).find(|&m| self.near.depth(self.sp.key(&self.sp.step(&s, m))) == Some(j)).expect("a level below");
            s = self.sp.step(&s, m);
            moves.push(m);
        }
        (Some(Way { frame: at, moves }), scored, kept)
    }
}

/// A cube as phase C leaves it, then `k` random moves, the first not one
/// that keeps the goal, none undoing or reordering the one before.
pub fn walk(k: usize, rnd: &mut impl FnMut(usize) -> usize) -> (Cube5, Vec<u8>) {
    let mut cube = Cube5::solved();
    for _ in 0..40 {
        cube.apply(G_MOVES[rnd(G_MOVES.len())]);
    }
    let mut seq: Vec<u8> = Vec::with_capacity(k);
    while seq.len() < k {
        let m = rnd(36) as u8;
        if seq.last().map_or(G_MOVES.contains(&m), |&last| skip_move(last, m)) {
            continue;
        }
        cube.apply(m);
        seq.push(m);
    }
    (cube, seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_network_reads_what_the_file_says() {
        // One unit in each layer: the inputs that are on add up, and each
        // layer passes what is above zero on.
        let mut bytes = b"S5N1".to_vec();
        for n in [1u32, 1, 1] {
            bytes.extend(n.to_le_bytes());
        }
        let w1: Vec<f32> = (0..N_IN).map(|i| if i < 96 { 0.5 } else { 0.0 }).collect();
        for v in w1.iter().chain(&[-10.0, 2.0, 1.0, 3.0, -4.0, 0.5, 7.0]) {
            bytes.extend(v.to_le_bytes());
        }
        let path = std::env::temp_dir().join(format!("split5-net-{}.bin", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let net = Net::load(path.to_str().unwrap()).unwrap();
        std::fs::remove_file(&path).unwrap();
        let on = inputs(&Cube5::solved());
        assert!(on.iter().enumerate().all(|(i, &v)| (v as usize) < N_IN && on[..i].iter().all(|&w| w != v)));
        // 24 x-centres at a half each, less ten: 2; doubled and one: 5; three times less four: 11.
        assert_eq!(net.value(&on), 0.5 * 11.0 + 7.0);
        let rec = record(&on, 3.0, true);
        assert_eq!(u16::from_le_bytes([rec[2], rec[3]]), on[1]);
        assert_eq!(f32::from_le_bytes(rec[2 * N_ON..2 * N_ON + 4].try_into().unwrap()), 3.0);
        assert_eq!(rec[2 * N_ON + 4], 1);
    }

    #[test]
    fn the_starts_are_cubes_the_moves_lead_to() {
        let tools = Tools::load(100_000, 200_000, 12);
        let (cube, _) = walk(9, &mut lcg(3));
        let starts = tools.starts(&[cube], 200, 3, 16);
        assert_eq!(starts.len(), 16);
        let best = starts.iter().map(|(_, moves)| {
            assert_eq!(moves.len(), 3);
            let mut c = cube;
            c.apply_seq(moves);
            tools.estimate(&c)
        });
        // Three moves on, the cubes kept are nearer by the estimate than the cube was.
        assert!(best.fold(f32::MAX, f32::min) < tools.estimate(&cube));
    }

    #[test]
    fn a_short_walk_is_found_and_ends_phase_c() {
        let tools = Tools::load(100_000, 200_000, 12);
        let mut rnd = lcg(5);
        for k in [0, 3, 6] {
            let (cube, _) = walk(k, &mut rnd);
            assert_eq!(tools.ended(&cube), k == 0);
            let (way, _) = tools.beam(&[cube], 2000, 12);
            let way = way.expect("a way of at most k moves");
            assert!(way.moves.len() <= k);
            let mut c = cube;
            c.apply_seq(&way.moves);
            assert!(tools.ended(&c));
        }
    }
}
