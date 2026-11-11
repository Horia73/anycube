//! Experiment: reductions of the 4x4x4 searched with all moves.
//!
//! A cube is reduced when its centres are solved in some orientation, its
//! dedges are paired and the 3x3x3 it shows is solvable. Which reductions a
//! cube has depends only on its class: the cubes reached from it by doing
//! face turns and whole-cube rotations *first*. `key` names the class.
//!
//! `Ball` holds every class within a few moves of the reduced one, so a
//! search forward from a cube can stop as soon as it steps into the ball.
//! Whole-cube rotations do not matter here, so 27 moves are enough: the face
//! turns and Uw, Rw, Fw (Dw is Uw and a rotation).

use crate::c3::C3;
use crate::cube::{perm_parity, skip_move, tables, Cube, SOLVED_CT};
use rayon::prelude::*;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;

/// Face turns and Uw, Rw, Fw.
pub const N_FREE: u8 = 27;

/// Keys are already hashes.
#[derive(Default)]
pub struct Plain(u64);

impl Hasher for Plain {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!()
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

type Map = HashMap<u64, u8, BuildHasherDefault<Plain>>;

/// Centres solved in some orientation, dedges paired, the 3x3x3 solvable.
pub fn is_reduced(c: &Cube) -> bool {
    let Some(home) = c.with_home_centres() else { return false };
    let Some(c3) = C3::from_cube(&home) else { return false };
    perm_parity(&c3.ep) == perm_parity(&c3.cp) && c3.eo.iter().map(|&e| e as u32).sum::<u32>() % 2 == 0
}

/// For each rotation, the colour that each colour becomes when the rotation is done first.
fn colour_maps() -> &'static [[u8; 6]; 24] {
    static M: OnceLock<[[u8; 6]; 24]> = OnceLock::new();
    M.get_or_init(|| {
        let t = tables();
        let mut maps = [[0u8; 6]; 24];
        for (r, map) in maps.iter_mut().enumerate() {
            for (colour, slot) in map.iter_mut().enumerate() {
                let home = SOLVED_CT.iter().position(|&c| c as usize == colour).unwrap();
                *slot = t.rots[r].state.ct[home];
            }
        }
        maps
    })
}

#[inline]
fn mix(h: u64, w: u64) -> u64 {
    let mut z = (h ^ w).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A 64-bit name for the class of `c`. Two cubes get the same key exactly when
/// one is the other preceded by face turns and rotations (up to hash collisions):
///
/// - the centres, recoloured by the rotation that makes them smallest;
/// - which wing positions hold the two halves of the same dedge;
/// - the parity of the wing permutation;
/// - the parity of the corners against that of the dedges, read in the order
///   in which the positions first show each dedge.
pub fn key(c: &Cube) -> u64 {
    let (centres, partner, bits) = class_parts(c);
    let mut h = bits as u64;
    for chunk in centres.chunks(8).chain(partner.chunks(8)) {
        h = mix(h, u64::from_le_bytes(chunk.try_into().unwrap()));
    }
    h
}

/// What `key` hashes: the recoloured centres, the position of each wing's
/// partner, and the two parities (bit 0 wings, bit 1 corners against dedges).
pub fn class_parts(c: &Cube) -> ([u8; 24], [u8; 24], u8) {
    let t = tables();
    let (mut best, mut best_ct) = (0usize, [u8::MAX; 24]);
    for (r, map) in colour_maps().iter().enumerate() {
        let mut ct = [0u8; 24];
        for i in 0..24 {
            ct[i] = map[c.ct[i] as usize];
        }
        if ct < best_ct {
            best = r;
            best_ct = ct;
        }
    }
    let x = c.after_rotation(&t.rots[best].state);
    let mut pos = [0u8; 24];
    for p in 0..24 {
        pos[x.ep[p] as usize] = p as u8;
    }
    let mut partner = [0u8; 24];
    let (mut seen, mut order, mut n) = ([false; 12], [0u8; 12], 0);
    for p in 0..24 {
        partner[p] = pos[(x.ep[p] as usize + 12) % 24];
        let dedge = (x.ep[p] % 12) as usize;
        if !seen[dedge] {
            seen[dedge] = true;
            order[n] = dedge as u8;
            n += 1;
        }
    }
    let bits = perm_parity(&x.ep) | (perm_parity(&order) ^ perm_parity(&x.cp)) << 1;
    (best_ct, partner, bits)
}

/// A 64-bit name for the cube itself, the same in all 24 orientations.
pub fn key_solved(c: &Cube) -> u64 {
    let t = tables();
    let (mut best, mut best_ct) = (0usize, [u8::MAX; 24]);
    for (r, map) in colour_maps().iter().enumerate() {
        let mut ct = [0u8; 24];
        for i in 0..24 {
            ct[i] = map[c.ct[i] as usize];
        }
        if ct < best_ct {
            best = r;
            best_ct = ct;
        }
    }
    let x = c.after_rotation(&t.rots[best].state);
    let mut h = 1u64;
    for part in [&x.ct[..], &x.ep[..], &x.cp[..], &x.co[..]] {
        for chunk in part.chunks(8) {
            h = mix(h, u64::from_le_bytes(chunk.try_into().unwrap()));
        }
    }
    h
}

/// Every class within `depth` moves of a goal, with its distance. The goal is
/// the reduced cubes (`build`) or the solved cube (`build_solved`).
pub struct Ball {
    map: Map,
    pub depth: u8,
    /// Classes at each distance.
    pub levels: Vec<usize>,
    key: fn(&Cube) -> u64,
    goal: fn(&Cube) -> bool,
}

impl Ball {
    /// Around the reduced cubes.
    pub fn build(depth: u8) -> Ball {
        Ball::build_with(depth, key, is_reduced)
    }

    /// Around the solved cube: whole solutions instead of reductions.
    pub fn build_solved(depth: u8) -> Ball {
        Ball::build_with(depth, key_solved, Cube::is_solved)
    }

    fn build_with(depth: u8, key: fn(&Cube) -> u64, goal: fn(&Cube) -> bool) -> Ball {
        let mut map = Map::default();
        map.insert(key(&Cube::solved()), 0);
        let mut frontier = vec![Cube::solved()];
        let mut levels = vec![1];
        for d in 1..=depth {
            let mut next = Vec::new();
            let mut count = 0usize;
            for chunk in frontier.chunks(1 << 15) {
                let known = &map;
                let cands: Vec<(u64, Cube)> = chunk
                    .par_iter()
                    .flat_map_iter(|c| {
                        (0..N_FREE).filter_map(move |m| {
                            let mut x = *c;
                            x.apply(m);
                            let k = key(&x);
                            (!known.contains_key(&k)).then_some((k, x))
                        })
                    })
                    .collect();
                for (k, x) in cands {
                    if let Entry::Vacant(e) = map.entry(k) {
                        e.insert(d);
                        count += 1;
                        // The last level is never expanded.
                        if d < depth {
                            next.push(x);
                        }
                    }
                }
            }
            levels.push(count);
            frontier = next;
        }
        Ball { map, depth, levels, key, goal }
    }

    /// Moves to a reduced cube, when that is at most `depth`.
    #[inline]
    pub fn dist(&self, c: &Cube) -> Option<u8> {
        self.map.get(&(self.key)(c)).copied()
    }

    /// A shortest reduction of a cube inside the ball; `None` if the walk down
    /// the distances does not end on a reduced cube, which would mean two
    /// classes share a key.
    pub fn descend(&self, c: &Cube) -> Option<Vec<u8>> {
        let mut x = *c;
        let mut d = self.dist(c)?;
        let mut path = Vec::new();
        while d > 0 {
            let (m, y) = (0..N_FREE).find_map(|m| {
                let mut y = x;
                y.apply(m);
                (self.dist(&y) == Some(d - 1)).then_some((m, y))
            })?;
            path.push(m);
            x = y;
            d -= 1;
        }
        (self.goal)(&x).then_some(path)
    }

    /// The shortest reduction of `cube` with all moves, if it has at most
    /// `forward + depth` moves: every sequence of f moves is tried for
    /// f = 0, 1, ..., and the first f at which one of them steps into the ball
    /// gives the answer.
    pub fn shortest(&self, cube: &Cube, forward: usize) -> Option<Vec<u8>> {
        for f in 0..=forward {
            let firsts: Vec<Option<u8>> = if f == 0 { vec![None] } else { (0..N_FREE).map(Some).collect() };
            let best = firsts
                .par_iter()
                .filter_map(|&first| {
                    let mut x = *cube;
                    let mut path = Vec::with_capacity(f);
                    let mut best: Option<(u8, Vec<u8>)> = None;
                    match first {
                        None => self.leaves(&x, 0, 255, &mut path, &mut best),
                        Some(m) => {
                            x.apply(m);
                            path.push(m);
                            self.leaves(&x, f - 1, m, &mut path, &mut best);
                        }
                    }
                    best
                })
                .min_by_key(|b| b.0);
            if let Some((_, mut path)) = best {
                let mut x = *cube;
                x.apply_seq(&path);
                path.extend(self.descend(&x)?);
                return Some(path);
            }
        }
        None
    }

    fn leaves(&self, c: &Cube, rem: usize, last: u8, path: &mut Vec<u8>, best: &mut Option<(u8, Vec<u8>)>) {
        if rem == 0 {
            if let Some(d) = self.dist(c) {
                if best.as_ref().is_none_or(|b| d < b.0) {
                    *best = Some((d, path.clone()));
                }
            }
            return;
        }
        for m in 0..N_FREE {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let mut x = *c;
            x.apply(m);
            path.push(m);
            self.leaves(&x, rem - 1, m, path, best);
            path.pop();
        }
    }
}

/// Beam search for a reduction with all moves. Every level keeps the `width`
/// cubes that look closest to a reduced cube and stops a path as soon as it
/// steps into the ball, where the rest is known exactly. "Looks close" is
/// `w_stage` times the staging distance of the centres (exact, best of the six
/// ways of giving the three colour pairs an axis) minus `w_paired` times the
/// number of dedges already paired.
pub struct Beam<'a> {
    pub ball: &'a Ball,
    pub solver: &'a crate::solver::Solver,
    pub width: usize,
    pub w_stage: i32,
    pub w_paired: i32,
    pub max_depth: usize,
}

struct Item {
    cube: Cube,
    /// Staging distance for each assignment of colour pairs to axes.
    stage: [u8; 6],
    last: u8,
}

struct Child {
    parent: u32,
    m: u8,
    stage: [u8; 6],
    score: i32,
}

/// (colour pair that goes to R/L, colour pair that goes to U/D), as indices of `axis_masks`.
const ROLES: [(usize, usize); 6] = [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)];

impl Beam<'_> {
    /// The reduction found, if any, and the number of cubes looked at.
    pub fn reduce(&self, cube: &Cube) -> (Option<Vec<u8>>, usize) {
        use crate::tables::axis_masks;
        let (ctab, tabs) = (&self.solver.ctab, &self.solver.tabs);
        if self.ball.dist(cube).is_some() {
            return (self.ball.descend(cube), 1);
        }
        let masks = axis_masks(cube);
        let root = Item {
            cube: *cube,
            stage: ROLES.map(|(a, b)| ctab.exact(&tabs.bp, &tabs.ranks, masks[a], masks[b])),
            last: 255,
        };
        let mut items = vec![root];
        // Per level: (parent, move) of every cube kept.
        let mut links: Vec<Vec<(u32, u8)>> = Vec::new();
        // Best so far: (total length, level of the parent, parent, move).
        let mut best: Option<(usize, usize, u32, u8)> = None;
        let mut looked = 1usize;
        for g in 0..self.max_depth {
            // Nothing shorter can turn up: a cube of the next level is g + 1 moves in and not reduced.
            if best.is_some_and(|b| b.0 <= g + 2) {
                break;
            }
            let found: Vec<(Vec<Child>, Option<(u8, u32, u8)>)> = items
                .par_chunks(256)
                .enumerate()
                .map(|(ci, chunk)| {
                    let mut out = Vec::with_capacity(chunk.len() * 21);
                    let mut hit: Option<(u8, u32, u8)> = None;
                    for (k, it) in chunk.iter().enumerate() {
                        let parent = (ci * 256 + k) as u32;
                        for m in 0..N_FREE {
                            if it.last != 255 && skip_move(it.last, m) {
                                continue;
                            }
                            let mut x = it.cube;
                            x.apply(m);
                            if let Some(d) = self.ball.dist(&x) {
                                if hit.is_none_or(|h| d < h.0) {
                                    hit = Some((d, parent, m));
                                }
                                continue;
                            }
                            let masks = axis_masks(&x);
                            let mut stage = it.stage;
                            for (i, &(a, b)) in ROLES.iter().enumerate() {
                                // The table holds distances modulo 3; one move changes them by at most 1.
                                let v = ctab.get(ctab.index(&tabs.ranks, masks[a], masks[b]));
                                match (v + 3 - stage[i] % 3) % 3 {
                                    1 => stage[i] += 1,
                                    2 => stage[i] -= 1,
                                    _ => {}
                                }
                            }
                            let paired = (0..12).filter(|&s| x.ep[s + 12] == (x.ep[s] + 12) % 24).count() as i32;
                            let score = self.w_stage * *stage.iter().min().unwrap() as i32 - self.w_paired * paired;
                            out.push(Child { parent, m, stage, score });
                        }
                    }
                    (out, hit)
                })
                .collect();
            let mut children: Vec<Child> = Vec::new();
            for (out, hit) in found {
                looked += out.len();
                if let Some((d, parent, m)) = hit {
                    let total = g + 1 + d as usize;
                    if best.is_none_or(|b| total < b.0) {
                        best = Some((total, g, parent, m));
                    }
                }
                children.extend(out);
            }
            if children.len() > self.width {
                children.select_nth_unstable_by_key(self.width, |c| c.score);
                children.truncate(self.width);
            }
            let next: Vec<Item> = children
                .par_iter()
                .map(|c| {
                    let mut x = items[c.parent as usize].cube;
                    x.apply(c.m);
                    Item { cube: x, stage: c.stage, last: c.m }
                })
                .collect();
            links.push(children.iter().map(|c| (c.parent, c.m)).collect());
            items = next;
        }
        let Some((_, level, mut parent, m)) = best else { return (None, looked) };
        let mut path = vec![m];
        for g in (0..level).rev() {
            let (p, mv) = links[g][parent as usize];
            path.push(mv);
            parent = p;
        }
        path.reverse();
        let mut x = *cube;
        x.apply_seq(&path);
        match self.ball.descend(&x) {
            Some(rest) => {
                path.extend(rest);
                (Some(path), looked)
            }
            None => (None, looked),
        }
    }
}
