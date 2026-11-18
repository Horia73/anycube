//! Re-enter the outer-turn subgroup from either side of a reduction.
//! Centre colours and wing partners determine whether the remaining cube
//! is a 3x3x3. The labels and orientations of the paired edges, and all
//! corners, are free to change. The finishing search prices that change.

use crate::c3::{C3, T3};
use crate::c5::{tables5, Cube5};
use crate::cube::{compose_src, inv_move, skip_move, tables, SOLVED_CT};
use crate::two3::{Effort, Two3};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct Pattern {
    x: [u8; 24],
    t: [u8; 24],
    // Oriented middle-edge slot that belongs to the wing in each slot.
    mate: [u8; 24],
}

impl Pattern {
    fn goal() -> Self {
        Self {
            x: SOLVED_CT,
            t: SOLVED_CT,
            mate: std::array::from_fn(|i| i as u8),
        }
    }
}

struct Endings {
    depth: HashMap<Pattern, u8>,
}

struct Turns {
    x: [[u8; 24]; 36],
    t: [[u8; 24]; 36],
    wing: [[u8; 24]; 36],
    mate_dest: [[u8; 24]; 36],
}

impl Turns {
    fn new() -> Self {
        let tb = tables();
        Self {
            x: tb.ct_src,
            t: std::array::from_fn(|m| tables5().t_src[m]),
            wing: tb.ep_src,
            mate_dest: std::array::from_fn(|m| tb.ep_src[inv_move((m % 18) as u8) as usize]),
        }
    }

    #[inline]
    fn step(&self, p: &Pattern, m: u8) -> Pattern {
        let m = m as usize;
        Pattern {
            x: compose_src(&p.x, &self.x[m]),
            t: compose_src(&p.t, &self.t[m]),
            mate: compose_src(&self.mate_dest[m], &compose_src(&p.mate, &self.wing[m])),
        }
    }
}

const BACK: u8 = 4;
const FORWARD: usize = 2;

impl Endings {
    fn build(turns: &Turns) -> Self {
        let goal = Pattern::goal();
        let mut depth = HashMap::from([(goal, 0)]);
        let mut fringe = vec![goal];
        for d in 1..=BACK {
            let mut next = Vec::new();
            for p in fringe {
                for m in 0..36 {
                    let child = turns.step(&p, m);
                    if let std::collections::hash_map::Entry::Vacant(e) = depth.entry(child) {
                        e.insert(d);
                        if d < BACK {
                            next.push(child);
                        }
                    }
                }
            }
            fringe = next;
        }
        Self { depth }
    }
}

fn book() -> &'static (Turns, Endings) {
    static ENDINGS: OnceLock<(Turns, Endings)> = OnceLock::new();
    ENDINGS.get_or_init(|| {
        let turns = Turns::new();
        let endings = Endings::build(&turns);
        (turns, endings)
    })
}

fn moved(mut c: Cube5, seq: &[u8]) -> Cube5 {
    c.apply_seq(seq);
    c
}

fn reduced(c: &Cube5) -> Option<C3> {
    (c.c4.ct == SOLVED_CT && c.t == SOLVED_CT && crate::edges5::is_reduced(c)).then_some(C3 {
        cp: c.c4.cp,
        co: c.c4.co,
        ep: c.mid.ep,
        eo: c.mid.eo,
    })
}

pub(crate) struct Route {
    pub moves: Vec<u8>,
    pub kept: usize,
    pub reduction: usize,
}

struct Probe<'a> {
    turns: &'a Turns,
    endings: &'a Endings,
    root: Cube5,
    prefix: &'a [u8],
    limit: usize,
    paths: &'a mut Vec<(usize, C3, Vec<u8>)>,
    seen: &'a mut HashMap<C3, usize>,
}

impl Probe<'_> {
    fn hit(&mut self, end: &[u8]) {
        let len = self.prefix.len() + end.len();
        let Some(c3) = reduced(&moved(self.root, end)) else { return };
        if self.seen.get(&c3).is_some_and(|&old| old <= len) {
            return;
        }
        self.seen.insert(c3, len);
        let mut path = self.prefix.to_vec();
        path.extend_from_slice(end);
        self.paths.push((self.prefix.len(), c3, path));
    }

    fn descend(&mut self, p: &Pattern, left: u8, word: &mut Vec<u8>, last: u8) {
        if left == 0 {
            self.hit(word);
            return;
        }
        for m in 0..36 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let child = self.turns.step(p, m);
            if self.endings.depth.get(&child) == Some(&(left - 1)) {
                word.push(m);
                self.descend(&child, left - 1, word, m);
                word.pop();
            }
        }
    }

    fn search(&mut self, p: &Pattern, forward: usize, word: &mut Vec<u8>, last: u8) {
        if let Some(&d) = self.endings.depth.get(p) {
            let shortest = word.len() + d as usize;
            if shortest > self.limit {
                return;
            }
            self.descend(p, d, word, last);
            // Every shortest continuation is in the table. Off the fringe
            // a step may still buy a different (and cheaper) 3x3x3.
            if d == 0 || shortest == self.limit {
                return;
            }
        } else if word.len() + BACK as usize >= self.limit {
            return;
        }
        if forward == 0 {
            return;
        }
        for m in 0..36 {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let child = self.turns.step(p, m);
            word.push(m);
            self.search(&child, forward - 1, word, m);
            word.pop();
        }
    }
}

/// The prefix and the old total are in this root's frame; `spent` is the
/// cost already paid on a larger cube before reaching it.
pub(crate) struct Seed {
    pub root: Cube5,
    pub prefix: Vec<u8>,
    pub spent: usize,
}

fn variants(seed: &Seed) -> Vec<(usize, C3, Vec<u8>)> {
    let root = seed.root;
    let moves = &seed.prefix;
    let Some(split) = moves.iter().rposition(|&m| m >= 18).map(|p| p + 1) else {
        return Vec::new();
    };
    let (turns, endings) = book();
    let mut targets = vec![Pattern::goal(); split + 1];
    for cut in (0..split).rev() {
        targets[cut] = turns.step(&targets[cut + 1], inv_move(moves[cut]));
    }
    let mut paths = Vec::new();
    let mut seen = HashMap::new();
    if let Some(c3) = reduced(&moved(root, &moves[..split])) {
        seen.insert(c3, split);
    }
    for gap in 1..=split.min(BACK as usize + FORWARD + 1) {
        let cut = split - gap;
        let prefix = &moves[..cut];
        let mut probe = Probe {
            turns,
            endings,
            root: moved(root, prefix),
            prefix,
            limit: (gap + 1).min(BACK as usize + FORWARD),
            paths: &mut paths,
            seen: &mut seen,
        };
        probe.search(&targets[cut], FORWARD.min(gap + 1), &mut Vec::with_capacity(8), 255);
    }
    // A cut may be far from the goal but quite close to another point of
    // this path in the quotient. Transfer a turn across that interval, or
    // omit it entirely, then price the 3x3x3 that is left.
    for cut in 0..split {
        for m in 0..=36u8 {
            let child = if m == 36 { targets[cut] } else { turns.step(&targets[cut], m) };
            for next in cut..=(cut + 5).min(split) {
                if next == cut && m == 36 || next == cut + 1 && m == moves[cut] || child != targets[next] {
                    continue;
                }
                let end = (m < 36).then_some(m);
                if end.is_some_and(|x| (cut > 0 && moves[cut - 1] / 3 == x / 3) || (next < split && moves[next] / 3 == x / 3)) {
                    continue;
                }
                let mut path = moves[..cut].to_vec();
                path.extend(end);
                path.extend_from_slice(&moves[next..split]);
                let Some(real_end) = path.iter().rposition(|&m| m >= 18) else {
                    continue;
                };
                path.truncate(real_end + 1);
                let Some(c3) = reduced(&moved(root, &path)) else { continue };
                if seen.get(&c3).is_some_and(|&old| old <= path.len()) {
                    continue;
                }
                seen.insert(c3, path.len());
                paths.push((cut.min(path.len()), c3, path));
            }
        }
    }
    paths
}

pub(crate) fn improve(seeds: &[Seed], best: usize, t3: &T3, two: &Two3) -> Option<(usize, Route)> {
    use rayon::prelude::*;
    let parts: Vec<_> = seeds.par_iter().map(variants).collect();
    let mut paths: Vec<_> = parts
        .into_iter()
        .enumerate()
        .flat_map(|(i, part)| {
            part.into_iter()
                .map(move |(cut, c3, path)| (seeds[i].spent + path.len(), i, cut, c3, path))
        })
        .collect();
    paths.sort_by_key(|p| p.0);
    let mut seen = HashSet::new();
    paths.retain(|p| seen.insert(p.3));
    // Spend finishing searches on distinct arrivals, with the shortest
    // reduction first. A strict bound preserves the incumbent on ties.
    paths.truncate(192);
    let result = paths
        .par_iter()
        .enumerate()
        .filter_map(|(k, (cost, _, _, c3, _))| {
            let cap = best.checked_sub(cost + 1)?;
            let tail = two.solve_within(t3, c3, Effort { depth: 11, variants: 6 }, cap)?;
            Some((cost + tail.len(), k, tail))
        })
        .min_by_key(|(total, k, _)| (*total, *k))?;
    let (_, i, cut, _, mut path) = paths.swap_remove(result.1);
    let reduction = path.len();
    path.extend(result.2);
    Some((
        i,
        Route {
            kept: cut,
            reduction,
            moves: path,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_tracks_both_centre_orbits_and_oriented_middle_edges() {
        let turns = Turns::new();
        let mut pattern = Pattern::goal();
        let mut cube = Cube5::solved();
        for i in 0..180 {
            let m = ((i * 17 + i / 7) % 36) as u8;
            pattern = turns.step(&pattern, m);
            cube.apply(m);
            assert_eq!((pattern.x, pattern.t), (cube.c4.ct, cube.t));
            for (slot, &wing) in cube.c4.ep.iter().enumerate() {
                let mid = cube.mid.ep.iter().position(|&edge| edge == wing % 12).unwrap();
                assert_eq!(pattern.mate[slot], mid as u8 + 12 * (cube.mid.eo[mid] ^ (wing / 12)));
            }
        }
    }
}
