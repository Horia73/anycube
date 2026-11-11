//! Matching the centres of one ring to the ring inside it, on a cube of any size.
//!
//! The solvers for 6x6x6 and 7x7x7 never bring the oblique centres home by
//! themselves. They make each one agree in colour with the centre next to
//! it towards the middle, and from then on the two travel together through
//! every move that does not cut between them; the edge phases, which solve
//! the inner centres anyway, bring the outer ones home with them. This is
//! the same for any size.
//!
//! Take row `a` of a face, counted from an edge, and the row below it. A
//! centre of row `a` and the centre under it are separated only by turns
//! exactly `a + 1` layers deep; turns that reach deeper or less deep move
//! both or neither. So if every centre of every row agrees with the one
//! under it, each column of centres from a diagonal centre up to the rim is
//! one colour, and it stays one colour under the turns of any small cube
//! made of layers at or below that diagonal centre.
//!
//! "Agrees" may be read the other way round as well, one axis at a time: on
//! the faces of an axis every centre of the row may show the colour opposite
//! to the one under it. When the row below is solved the row is then one
//! colour all the same, only on the opposite face, which the edge step of
//! its own ring puts right. That is what the 7x7x7 solver's matching allows.
//!
//! The rows are done from the rim inwards: row `a` needs turns `a + 1`
//! layers deep and deeper, which leave alone what the rows nearer the rim
//! have achieved. The centres must be staged already (each on a face of its
//! own axis), and stay so: only face turns and deep half turns are used.
//!
//! Within a row the columns are done from the middle of the row to its
//! ends, a column and its mirror image at a time. A turn `d` layers deep
//! about a face beside the row cuts the row after its first `d - 1`
//! centres, so the columns at the ends can be worked on with shallow cuts
//! that leave the columns nearer the middle whole: those then only ride
//! along with the turns that separate the row from the one under it, and
//! the search has to bring them back.
//!
//! Three ways are tried:
//!
//! - all columns at once, a beam search whose score adds, over the columns
//!   and the three axes, the exact number of half moves until the patterns
//!   of a centre and the one under it agree, each move's cost shared
//!   between the axes. The shortest, but it works on short rows only: all
//!   columns hang together through the turns that separate the two rows,
//!   and a sum over the columns does not see that;
//! - from the middle of the row outwards (`extend`), each step a search of
//!   the same small size however long the row is. This is what makes rows
//!   of any length possible;
//! - column by column with plain moves, the columns done before followed
//!   and brought back: a last resort.

use crate::cuben::Geometry;
use crate::search::{beam_from, Problem};
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes};
use rayon::prelude::*;

const FAR: u8 = 255;
/// Centres followed at once: twice the length of a row.
const MAX: usize = 48;
/// The longest row that is tried with all its columns in one search.
const WHOLE: usize = 4;
/// States per level of the beam searches.
const WIDTH: usize = 30_000;

type Node = [u32; MAX];

pub struct Ring {
    n: usize,
    row: usize,
    /// How far from the nearer end of a face each column is.
    level: Vec<usize>,
    /// Moves of the cube: face turns, and half turns from `a + 1` layers deep to the middle.
    moves: Vec<u8>,
    /// Per column, the facelets of the centre in row `a` and of the one
    /// under it, by position: eight positions per axis, four per face.
    cells: Vec<[usize; 24]>,
    perms: Vec<BitPerm>,
    rank8: [u8; 256],
    unrank8: Vec<u32>,
    /// Tables of `group_table` made so far.
    tables: std::sync::Mutex<std::collections::HashMap<(usize, Vec<usize>, Vec<bool>, usize), std::sync::Arc<Bytes>>>,
}

/// One step: the columns from some level to the middle.
struct Step<'a> {
    ring: &'a Ring,
    columns: Vec<usize>,
    /// Per column of the row and axis: half moves until the two patterns
    /// agree, and until they are each other's opposite.
    dist: &'a [[[Vec<u8>; 2]; 3]],
    /// Per axis: 0 if the patterns are to agree, 1 if they are to be opposite.
    signs: [usize; 3],
}

/// Axis of a face in the numbering of the positions (U/D, F/B, R/L), from its number among U R F D L B.
fn axis_of(face: usize) -> usize {
    [0, 2, 1][face % 3]
}

impl Ring {
    pub fn new(g: &Geometry, a: usize) -> Ring {
        let n = g.n;
        assert!(a >= 1 && 2 * a + 4 <= n, "a ring with another ring inside it");
        let columns: Vec<usize> = (a + 1..n - 1 - a).collect();
        assert!(2 * columns.len() <= MAX, "the cube is too large for this table of centres");
        let moves: Vec<u8> = (0..g.move_count())
            .filter(|m| m / 18 == 0 || (m % 3 == 1 && m / 18 >= a))
            .map(|m| u8::try_from(m).expect("move numbers fit a byte up to size 28"))
            .collect();
        // The four turned copies of a cell on each face, in the same order for every cell.
        let orbit = |row: usize, col: usize| -> [usize; 24] {
            let mut cells = [0usize; 24];
            for face in 0..6 {
                let (mut r, mut c) = (row, col);
                for turn in 0..4 {
                    cells[axis_of(face) * 8 + face / 3 * 4 + turn] = face * n * n + r * n + c;
                    (r, c) = (c, n - 1 - r);
                }
            }
            cells
        };
        let mut cells = Vec::new();
        for &c in &columns {
            cells.push(orbit(a, c));
            cells.push(orbit(a + 1, c));
        }
        let perms: Vec<BitPerm> = cells
            .iter()
            .map(|cells| {
                let mut position = vec![255u8; 6 * n * n];
                for (p, &facelet) in cells.iter().enumerate() {
                    position[facelet] = p as u8;
                }
                let src: Vec<[u8; 24]> = moves
                    .iter()
                    .map(|&m| {
                        let from = g.move_src(m as usize);
                        let mut row = [0u8; 24];
                        for p in 0..24 {
                            row[p] = position[from[cells[p]] as usize];
                            assert!(row[p] != 255, "a move keeps an orbit of centres to itself");
                        }
                        row
                    })
                    .collect();
                BitPerm::from_src(&src)
            })
            .collect();
        let mut rank8 = [255u8; 256];
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        for (i, &x) in unrank8.iter().enumerate() {
            rank8[x as usize] = i as u8;
        }
        let level = columns.iter().map(|&c| c.min(n - 1 - c)).collect();
        Ring { n, row: a, level, moves, cells, perms, rank8, unrank8, tables: Default::default() }
    }

    /// The matching built from the middle of the row outwards, so that each
    /// step is a search of the same small size however long the row is.
    ///
    /// The columns done so far are kept not only matched but alike: the
    /// same pattern in the row and the same pattern in the row under it.
    /// Columns that are alike and are not cut apart move alike, so however
    /// many they are, they are one thing to follow, and a step has to deal
    /// with that one column and the one or two columns it adds. A step uses
    /// cuts no deeper than its own columns, which leave the columns nearer
    /// the middle uncut. The two columns at the ends of the row have only
    /// to be matched, not made like the rest: nothing is added after them.
    fn extend(&self, g: &Geometry, f: &[u8], signs: [usize; 3]) -> Option<Vec<u8>> {
        let deepest = *self.level.iter().max().unwrap();
        let at = |level: usize| (0..self.level.len()).filter(|&k| self.level[k] == level).collect::<Vec<_>>();
        let like = at(deepest)[0];
        let mut f = f.to_vec();
        let mut out: Vec<u8> = Vec::new();
        for level in (self.row + 1..=deepest).rev() {
            let places: Vec<usize> = (0..self.moves.len()).filter(|&i| self.moves[i] as usize / 18 <= level).collect();
            let ids: Vec<u8> = places.iter().map(|&i| self.moves[i]).collect();
            // Groups of orbits whose patterns must all agree, each orbit
            // with whether it is in the row below (where the sign of the axis applies).
            let (a_like, b_like) = (2 * like, 2 * like + 1);
            let new: Vec<usize> = at(level).into_iter().filter(|&k| k != like).collect();
            let mut groups: Vec<Vec<(usize, bool)>> = Vec::new();
            if level == self.row + 1 {
                // The ends: only matched.
                groups.push(vec![(a_like, false), (b_like, true)]);
                groups.extend(new.iter().map(|&k| vec![(2 * k, false), (2 * k + 1, true)]));
            } else if new.is_empty() {
                groups.push(vec![(a_like, false), (b_like, true)]);
            } else {
                // Each new column like the first one, in both rows.
                groups.extend(new.iter().map(|&k| vec![(a_like, false), (b_like, true), (2 * k, false), (2 * k + 1, true)]));
            }
            // The tables of a step, made side by side where they have to be made.
            let wanted: Vec<(usize, usize)> = (0..groups.len()).flat_map(|i| (0..3).map(move |axis| (i, axis))).collect();
            let made: Vec<std::sync::Arc<Bytes>> = wanted
                .par_iter()
                .map(|&(i, axis)| {
                    let orbits: Vec<usize> = groups[i].iter().map(|x| x.0).collect();
                    let flips: Vec<bool> = groups[i].iter().map(|x| x.1 && signs[axis] == 1).collect();
                    self.group_table(level, &ids, &places, &orbits, &flips, axis)
                })
                .collect();
            let dist: Vec<[std::sync::Arc<Bytes>; 3]> =
                (0..groups.len()).map(|i| [made[3 * i].clone(), made[3 * i + 1].clone(), made[3 * i + 2].clone()]).collect();
            let mut orbits: Vec<usize> = groups.iter().flatten().map(|x| x.0).collect();
            orbits.sort();
            orbits.dedup();
            let grow = Grow { ring: self, ids, places, orbits, groups: groups.iter().map(|g| g.iter().map(|x| x.0).collect()).collect(), dist };
            let from = self.node(&f).ok()?;
            let score = |n: &Node| grow.halves(n);
            let found = beam_from(&grow, std::slice::from_ref(&from), score, WIDTH, 1, 50, 0, &|_| true).into_iter().next();
            if std::env::var_os("CUBEN_TRACE").is_some() {
                eprintln!(
                    "      level {}: {} groups of orbits, {} moves to choose from, score {} -> {:?} moves",
                    level,
                    grow.groups.len(),
                    grow.ids.len(),
                    grow.halves(&from),
                    found.as_ref().map(|m| m.len())
                );
            }
            let moves = found?;
            g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            out.extend(moves);
        }
        Some(out)
    }

    /// For some orbits and one axis: half moves until their patterns agree
    /// (those marked taken as their opposites), with the moves given and
    /// each move's cost shared between the axes. Kept for the next cube.
    fn group_table(&self, level: usize, ids: &[u8], places: &[usize], orbits: &[usize], flips: &[bool], axis: usize) -> std::sync::Arc<Bytes> {
        let key = (level, orbits.to_vec(), flips.to_vec(), axis);
        if let Some(table) = self.tables.lock().unwrap().get(&key) {
            return table.clone();
        }
        let size = 70usize.pow(orbits.len() as u32);
        let bytes = match orbits.len() {
            // The large ones take seconds to make and are kept as files.
            4 => {
                let orbit_names: Vec<String> = orbits.iter().map(|j| j.to_string()).collect();
                let flip_names: String = flips.iter().map(|&x| if x { '1' } else { '0' }).collect();
                let name = format!("ring-{}-{}-{}-{}-{}-{}.bin", self.n, self.row, level, orbit_names.join("_"), flip_names, axis);
                load_or_gen(&table_dir().join(name), size, || self.make_group_table(ids, places, orbits, flips, axis))
            }
            _ => Bytes::Owned(self.make_group_table(ids, places, orbits, flips, axis)),
        };
        let table = std::sync::Arc::new(bytes);
        self.tables.lock().unwrap().insert(key, table.clone());
        table
    }

    fn make_group_table(&self, ids: &[u8], places: &[usize], orbits: &[usize], flips: &[bool], axis: usize) -> Vec<u8> {
        let shift = 8 * axis as u32;
        let cost: Vec<u8> = ids
            .iter()
            .map(|&m| match (m >= 18, axis_of(m as usize / 3 % 6) == axis) {
                (false, true) => 2,
                (true, false) => 1,
                _ => 0,
            })
            .collect();
        // Moves that cannot change this axis at all are left out.
        let used: Vec<usize> = (0..ids.len()).filter(|&i| cost[i] > 0 || ids[i] >= 18).collect();
        let costs: Vec<usize> = used.iter().map(|&i| cost[i] as usize).collect();
        // Per orbit: where each move takes each pattern, already as that orbit's part of the index.
        let (k, m) = (orbits.len(), used.len());
        let mut to = vec![0u32; k * 70 * m];
        for (o, &j) in orbits.iter().enumerate() {
            for r in 0..70 {
                for (i, &u) in used.iter().enumerate() {
                    let pattern = self.perms[j].apply(places[u] as u8, self.unrank8[r] << shift) >> shift;
                    to[(o * 70 + r) * m + i] = self.rank8[pattern as usize] as u32 * 70u32.pow(o as u32);
                }
            }
        }
        let size = 70usize.pow(k as u32);
        let mut dist = vec![FAR; size];
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new()];
        for r in 0..70 {
            let opposite = self.rank8[(!self.unrank8[r] & 255) as usize] as usize;
            let goal = (0..k).rev().fold(0, |at, o| at * 70 + if flips[o] { opposite } else { r });
            dist[goal] = 0;
            buckets[0].push(goal as u32);
        }
        let mut rows = vec![0usize; k];
        let mut d = 0;
        while d < buckets.len() {
            let mut at = 0;
            while at < buckets[d].len() {
                let s = buckets[d][at] as usize;
                at += 1;
                if dist[s] as usize != d {
                    continue;
                }
                let mut rest = s;
                for (o, row) in rows.iter_mut().enumerate() {
                    *row = (o * 70 + rest % 70) * m;
                    rest /= 70;
                }
                for i in 0..m {
                    let mut t = 0u32;
                    for &row in &rows {
                        t += to[row + i];
                    }
                    let nd = d + costs[i];
                    if (dist[t as usize] as usize) > nd {
                        dist[t as usize] = nd as u8;
                        while buckets.len() <= nd {
                            buckets.push(Vec::new());
                        }
                        buckets[nd].push(t);
                    }
                }
            }
            buckets[d] = Vec::new();
            d += 1;
        }
        dist
    }

    /// Whether, on each axis, every centre of the row agrees with the one
    /// under it or every one is its opposite.
    pub fn matched(&self, f: &[u8]) -> bool {
        let Ok(n) = self.node(f) else { return false };
        (0..3).all(|axis| {
            let byte = |k: usize| n[k] >> (8 * axis) & 255;
            let all = |flip: u32| (0..self.level.len()).all(|k| byte(2 * k) == byte(2 * k + 1) ^ flip);
            all(0) || all(255)
        })
    }

    /// How many centres the row has.
    pub fn len(&self) -> usize {
        self.level.len()
    }

    fn node(&self, f: &[u8]) -> Result<Node, String> {
        let mut n = [0u32; MAX];
        for (k, cells) in self.cells.iter().enumerate() {
            n[k] = (0..24).fold(0u32, |m, i| m | ((f[cells[i]] < 3) as u32) << i);
            if (0..3).any(|axis| (n[k] >> (8 * axis) & 255).count_ones() != 4) {
                return Err("the centres must be staged before they are matched".to_string());
            }
        }
        Ok(n)
    }

    /// Moves of the cube after which, on each axis, every centre of the row
    /// agrees with the one under it or every one is its opposite; from a
    /// cube with staged centres.
    ///
    /// Which of the two it is to be on each axis is chosen before the
    /// search (a score that takes the nearer of the two at every state
    /// keeps a beam undecided, and it then finds nothing on most cubes).
    /// The eight choices are tried from the most promising one, with each
    /// of the three ways in turn.
    pub fn solve(&self, g: &Geometry, f: &[u8]) -> Result<Vec<u8>, String> {
        let root = self.node(f)?;
        let deepest = *self.level.iter().max().unwrap();
        let all: Vec<usize> = (0..self.level.len()).collect();
        let every: Vec<usize> = (0..self.moves.len()).collect();
        let table = |k: usize, axis: usize| {
            [false, true].map(|opposite| {
                pair_table(&self.moves, &every, &self.perms[2 * k], &self.perms[2 * k + 1], &self.rank8, &self.unrank8, axis, opposite)
            })
        };
        let dist: Vec<[[Vec<u8>; 2]; 3]> = all.iter().map(|&k| [0, 1, 2].map(|axis| table(k, axis))).collect();
        let step = |columns: Vec<usize>, signs: [usize; 3]| Step { ring: self, columns, dist: &dist, signs };
        let mut choices: Vec<(u32, [usize; 3])> = (0..8usize)
            .map(|bits| {
                let signs = [bits & 1, bits >> 1 & 1, bits >> 2];
                (step(all.clone(), signs).halves(&root), signs)
            })
            .collect();
        choices.sort();
        let trace = std::env::var_os("CUBEN_TRACE").is_some();
        let clock = std::time::Instant::now();
        let search = |step: &Step, from: &Node| {
            let score = |n: &Node| step.halves(n);
            beam_from(step, std::slice::from_ref(from), score, WIDTH, 1, 60, 0, &|_| true).into_iter().next()
        };
        // All columns at once: the shortest where it works, which is on short rows.
        if all.len() <= WHOLE {
            for (number, &(_, signs)) in choices.iter().enumerate() {
                if let Some(moves) = search(&step(all.clone(), signs), &root) {
                    if trace {
                        eprintln!("    row {}: all columns at once, choice {}, {} moves, {:.1} s", self.row, number, moves.len(), clock.elapsed().as_secs_f64());
                    }
                    return Ok(moves);
                }
            }
        }
        for (number, &(_, signs)) in choices.iter().enumerate() {
            if let Some(moves) = self.extend(g, f, signs) {
                if trace {
                    eprintln!("    row {}: from the middle outwards, choice {}, {} moves, {:.1} s", self.row, number, moves.len(), clock.elapsed().as_secs_f64());
                }
                return Ok(moves);
            }
        }
        // Column by column with every column done so far followed.
        for (number, &(_, signs)) in choices.iter().enumerate() {
            let mut f = f.to_vec();
            let mut out: Vec<u8> = Vec::new();
            let done = (self.row + 1..=deepest).rev().all(|level| {
                let columns: Vec<usize> = all.iter().copied().filter(|&k| self.level[k] >= level).collect();
                let Ok(from) = self.node(&f) else { return false };
                match search(&step(columns, signs), &from) {
                    Some(moves) => {
                        g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
                        out.extend(moves);
                        true
                    }
                    None => false,
                }
            });
            if done {
                if trace {
                    eprintln!("    row {}: column by column, choice {}, {} moves, {:.1} s", self.row, number, out.len(), clock.elapsed().as_secs_f64());
                }
                return Ok(out);
            }
        }
        Err(format!("no matching found for the centres of layer {}", self.row + 1))
    }
}

/// One step of `extend`: groups of orbits whose patterns must come to agree.
struct Grow<'a> {
    ring: &'a Ring,
    ids: Vec<u8>,
    /// The moves as places among the ring's moves.
    places: Vec<usize>,
    orbits: Vec<usize>,
    groups: Vec<Vec<usize>>,
    /// Per group and axis: half moves until the patterns of the group agree.
    dist: Vec<[std::sync::Arc<Bytes>; 3]>,
}

impl Grow<'_> {
    #[inline(always)]
    fn halves(&self, n: &Node) -> u32 {
        let mut sum = 0u32;
        for (group, dist) in self.groups.iter().zip(&self.dist) {
            for axis in 0..3 {
                let mut at = 0;
                for &j in group.iter().rev() {
                    at = at * 70 + self.ring.rank8[(n[j] >> (8 * axis) & 255) as usize] as usize;
                }
                sum += dist[axis][at] as u32;
            }
        }
        sum
    }
}

impl Problem for Grow<'_> {
    type Node = Node;

    fn moves(&self) -> &[u8] {
        &self.ids
    }

    fn next(&self, n: &Node, k: usize) -> Node {
        let mut out = *n;
        for &j in &self.orbits {
            out[j] = self.ring.perms[j].apply(self.places[k] as u8, n[j]);
        }
        out
    }

    fn bound(&self, n: &Node) -> usize {
        (self.halves(n) > 0) as usize
    }

    fn done(&self, _: &Node) -> bool {
        true
    }
}

impl Step<'_> {
    /// Half moves still needed, added over the columns and the axes.
    #[inline(always)]
    fn halves(&self, n: &Node) -> u32 {
        let mut sum = 0u32;
        for axis in 0..3 {
            let rank = |m: u32| self.ring.rank8[(m >> (8 * axis) & 255) as usize] as usize;
            for &k in &self.columns {
                sum += self.dist[k][axis][self.signs[axis]][rank(n[2 * k]) * 70 + rank(n[2 * k + 1])] as u32;
            }
        }
        sum
    }
}

impl Problem for Step<'_> {
    type Node = Node;

    fn moves(&self) -> &[u8] {
        &self.ring.moves
    }

    fn next(&self, n: &Node, k: usize) -> Node {
        let mut out = *n;
        for &c in &self.columns {
            for j in [2 * c, 2 * c + 1] {
                out[j] = self.ring.perms[j].apply(k as u8, n[j]);
            }
        }
        out
    }

    fn bound(&self, n: &Node) -> usize {
        (self.halves(n) > 0) as usize
    }

    fn done(&self, _: &Node) -> bool {
        true
    }
}

/// For two orbits and one axis: half moves until their patterns agree (or
/// are each other's opposite), with a face turn counted for its own axis
/// and a deep half turn half for each of the other two.
fn pair_table(
    moves: &[u8],
    places: &[usize],
    x: &BitPerm,
    y: &BitPerm,
    rank8: &[u8; 256],
    unrank8: &[u32],
    axis: usize,
    opposite: bool,
) -> Vec<u8> {
    let shift = 8 * axis as u32;
    let table = |perm: &BitPerm| -> Vec<Vec<u8>> {
        (0..70)
            .map(|r| {
                (0..moves.len())
                    .map(|i| {
                        let to = perm.apply(places[i] as u8, unrank8[r] << shift) >> shift;
                        assert!(to < 256 && rank8[to as usize] != 255, "these moves keep the centres staged");
                        rank8[to as usize]
                    })
                    .collect()
            })
            .collect()
    };
    let (mx, my) = (table(x), table(y));
    let cost: Vec<u8> = moves
        .iter()
        .map(|&m| match (m >= 18, axis_of(m as usize / 3 % 6) == axis) {
            (false, true) => 2,
            (true, false) => 1,
            _ => 0,
        })
        .collect();
    let mut dist = vec![FAR; 4900];
    let goals: Vec<u32> = (0..70)
        .map(|r| match opposite {
            true => (r * 70 + rank8[(!unrank8[r] & 255) as usize] as usize) as u32,
            false => (r * 71) as u32,
        })
        .collect();
    for &s in &goals {
        dist[s as usize] = 0;
    }
    let mut buckets: Vec<Vec<u32>> = vec![goals];
    let mut d = 0;
    while d < buckets.len() {
        let mut at = 0;
        while at < buckets[d].len() {
            let s = buckets[d][at] as usize;
            at += 1;
            if dist[s] as usize != d {
                continue;
            }
            for i in 0..moves.len() {
                let t = mx[s / 70][i] as usize * 70 + my[s % 70][i] as usize;
                let nd = d + cost[i] as usize;
                if (dist[t] as usize) > nd {
                    dist[t] = nd as u8;
                    while buckets.len() <= nd {
                        buckets.push(Vec::new());
                    }
                    buckets[nd].push(t as u32);
                }
            }
        }
        d += 1;
    }
    assert!(dist.iter().all(|&x| x != FAR), "any two staged patterns can be made to agree");
    dist
}
