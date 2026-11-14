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
//!   and a sum over the columns does not see that. For the same reason it
//!   does not arrive by itself, and its last moves are searched exactly
//!   (`Ending`);
//! - from the middle of the row outwards (`extend`), each step a search of
//!   the same small size however long the row is. This is what makes rows
//!   of any length possible;
//! - column by column with plain moves, the columns done before followed
//!   and brought back: a last resort.

use crate::cuben::Geometry;
use crate::search::{beam_from, mixed, Problem};
use crate::stage6::skip54;
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes};
use rayon::prelude::*;

const FAR: u8 = 255;
/// Centres followed at once: twice the length of a row.
const MAX: usize = 48;
/// The longest row that is tried with all its columns in one search.
const WHOLE: usize = 5;
/// States per level of the beam searches.
const WIDTH: usize = 30_000;
/// The longest matching looked for.
const LONGEST: usize = 60;
/// Centres that may still be apart from the one under them in a state
/// that is tried for an ending (`Ending`): two pairs.
const APART: u32 = 4;
/// Such states taken per choice of signs and length.
const NEAR: usize = 48;
/// Moves before the separating turn that are tried for each of them.
const EASY: usize = 5;
/// Lengths the searches go on for after the first matching.
const LONGER: usize = 1;
/// Levels on which every state of a search may have one and the same
/// score before that search is given up.
const PATIENCE: usize = 8;
/// Should no ending turn up that way: the most moves before the turn, and
/// the positions one try may then visit.
const END: usize = 7;
const END_BUDGET: u64 = 2_000_000;

type Node = [u32; MAX];

/// The tables kept as files, by fingerprint: whoever asks first makes or
/// loads one, the others wait for it.
type Slot = std::sync::Arc<std::sync::OnceLock<std::sync::Arc<Bytes>>>;
static SHARED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<u64, Slot>>> = std::sync::LazyLock::new(Default::default);

pub struct Ring {
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
        Ring { row: a, level, moves, cells, perms, rank8, unrank8, tables: Default::default() }
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
        let (to, costs) = self.group_moves(ids, places, orbits, axis);
        let table = match orbits.len() {
            // The large ones take seconds to make and are kept as files,
            // named by what they are made from: rows, sizes and axes whose
            // orbits are moved alike share one file, and one copy in memory.
            4 => {
                let print = fingerprint(&to, &costs, flips);
                let slot = SHARED.lock().unwrap().entry(print).or_default().clone();
                slot.get_or_init(|| {
                    let name = format!("ring-{:016x}.bin", print);
                    std::sync::Arc::new(load_or_gen(&table_dir().join(name), size, || self.make_group_table(&to, &costs, flips)))
                })
                .clone()
            }
            _ => std::sync::Arc::new(Bytes::Owned(self.make_group_table(&to, &costs, flips))),
        };
        self.tables.lock().unwrap().insert(key, table.clone());
        table
    }

    /// What the moves do to some orbits on one axis, and what each costs
    /// there: per move, where it takes each pattern of each orbit (already
    /// as that orbit's part of the table's index). Moves that change
    /// nothing are left out, moves that do the same are one, and the order
    /// is the same whatever order the moves came in.
    fn group_moves(&self, ids: &[u8], places: &[usize], orbits: &[usize], axis: usize) -> (Vec<Vec<u32>>, Vec<usize>) {
        let shift = 8 * axis as u32;
        let mut moves: Vec<(Vec<u32>, usize)> = Vec::new();
        for (i, &id) in ids.iter().enumerate() {
            let cost = match (id >= 18, axis_of(id as usize / 3 % 6) == axis) {
                (false, true) => 2,
                (true, false) => 1,
                // A move that cannot change this axis at all.
                (false, false) => continue,
                (true, true) => 0,
            };
            let mut to = Vec::with_capacity(orbits.len() * 70);
            let mut still = true;
            for (o, &j) in orbits.iter().enumerate() {
                for r in 0..70 {
                    let pattern = self.perms[j].apply(places[i] as u8, self.unrank8[r] << shift) >> shift;
                    let rank = self.rank8[pattern as usize] as usize;
                    still &= rank == r;
                    to.push(rank as u32 * 70u32.pow(o as u32));
                }
            }
            if !still {
                moves.push((to, cost));
            }
        }
        moves.sort();
        // Of moves that do the same, the first is the cheapest.
        moves.dedup_by(|b, a| a.0 == b.0);
        moves.into_iter().unzip()
    }

    fn make_group_table(&self, to: &[Vec<u32>], costs: &[usize], flips: &[bool]) -> Vec<u8> {
        let (k, m) = (flips.len(), to.len());
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
                    *row = o * 70 + rest % 70;
                    rest /= 70;
                }
                for i in 0..m {
                    let mut t = 0u32;
                    for &row in &rows {
                        t += to[i][row];
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

    /// All columns of a short row in one search: the choice of signs it
    /// came from, the moves, and how many of them are its ending.
    ///
    /// One beam search per choice of signs, all of them a level at a time.
    /// A search by itself ends as often as not among states that are one
    /// separating turn from done by its score and many moves in fact (see
    /// `Ending`), so the searches are not asked to arrive: on every level,
    /// the states of each with at most two pairs of centres left are tried
    /// for an ending of a few moves, and the shortest matching of all the
    /// choices is kept. They stop a level after the first one.
    fn whole(&self, root: &Node, dist: &[[[Vec<u8>; 2]; 3]], choices: &[(u32, [usize; 3])]) -> Option<(usize, Vec<u8>, usize)> {
        let short = Short::new(self, dist);
        let ending = Ending::new(&short);
        let from = short.ranks(root);
        let mut lanes: Vec<Lane> = choices.iter().enumerate().map(|(choice, &(_, signs))| Lane::new(&short, choice, signs, from)).collect();
        let mut best: Option<(usize, Vec<u8>, usize)> = None;
        let mut since = 0;
        // Should no ending of a few moves turn up: the first states tried, for a longer one.
        let mut early: Vec<Start> = Vec::new();
        for len in 1..=LONGEST {
            lanes.par_iter_mut().for_each(Lane::advance);
            let starts: Vec<Start> = lanes.iter().flat_map(Lane::starts).collect();
            let mut cap = best.as_ref().map_or(usize::MAX, |b| b.1.len());
            if let Some(done) = starts.iter().find(|s| s.apart == 0) {
                if len < cap {
                    (best, cap) = (Some((done.choice, done.moves.clone(), 0)), len);
                }
            }
            for depth in 0..=EASY {
                if len + depth + 1 >= cap {
                    break;
                }
                if let Some((choice, moves)) = ending.search(&starts, depth, u64::MAX) {
                    best = Some((choice, moves, depth + 1));
                    break;
                }
            }
            match &best {
                Some(found) => {
                    since += 1;
                    // What comes later is longer by now, or will be.
                    if len + 1 >= found.1.len() || since > LONGER {
                        break;
                    }
                }
                None if early.len() < 16 * NEAR => early.extend(starts),
                None => (),
            }
            if lanes.iter().all(|lane| lane.over) {
                break;
            }
        }
        // No ending of a few moves anywhere: longer ones from the first
        // states that were tried, the shortest matching first.
        best.or_else(|| {
            let shortest = early.iter().map(|s| s.moves.len()).min()?;
            (shortest + EASY + 2..=LONGEST).find_map(|total| {
                (EASY + 1..=END).find_map(|depth| {
                    let of: Vec<Start> = early.iter().filter(|s| s.moves.len() + depth + 1 == total).cloned().collect();
                    ending.search(&of, depth, END_BUDGET).map(|(choice, moves)| (choice, moves, depth + 1))
                })
            })
        })
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
        // All columns at once: the shortest, on short rows.
        if all.len() <= WHOLE {
            if let Some((number, moves, ending)) = self.whole(&root, &dist, &choices) {
                if trace {
                    eprintln!(
                        "    row {}: all columns at once, choice {}, {} moves (the last {} an ending), {:.1} s",
                        self.row,
                        number,
                        moves.len(),
                        ending,
                        clock.elapsed().as_secs_f64()
                    );
                }
                return Ok(moves);
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

/// A short row for the search of all its columns at once (`Ring::whole`).
///
/// Its states are held as ranks: per column and axis (a "slot") the
/// pattern of the row and the pattern of the row under it, a byte each.
/// A move is then two small lookups a slot, and a state is 30 bytes to
/// copy, hash and sort where `Node` is 192.
struct Short<'a> {
    ring: &'a Ring,
    /// Three for each column.
    slots: usize,
    /// Per slot, for the row and for the row under it, and per move of the
    /// ring: where the move takes each pattern.
    to: Vec<[Vec<[u8; 70]>; 2]>,
    /// Per slot, sign and pair of patterns, for every move of the ring
    /// side by side: the half moves still needed after it, until the two
    /// patterns agree (sign 0) or are each other's opposite (sign 1), as
    /// `Step` has them. The scores of all the moves from a state are then
    /// the sum of one such row per slot.
    after: Vec<u8>,
}

/// The patterns of a short row: `2 * slot` for the row, `2 * slot + 1` for the row under it.
type Ranks = [u8; 6 * WHOLE];

impl<'a> Short<'a> {
    fn new(ring: &'a Ring, dist: &'a [[[Vec<u8>; 2]; 3]]) -> Short<'a> {
        let columns = ring.level.len();
        assert!(columns <= WHOLE);
        let map = |orbit: usize, axis: usize, k: usize| -> [u8; 70] {
            let shift = 8 * axis as u32;
            std::array::from_fn(|r| ring.rank8[(ring.perms[orbit].apply(k as u8, ring.unrank8[r] << shift) >> shift) as usize])
        };
        let slots: Vec<(usize, usize)> = (0..columns).flat_map(|c| (0..3).map(move |axis| (c, axis))).collect();
        let count = ring.moves.len();
        let to: Vec<[Vec<[u8; 70]>; 2]> =
            slots.iter().map(|&(c, axis)| [0, 1].map(|under| (0..count).map(|k| map(2 * c + under, axis, k)).collect())).collect();
        let mut after = vec![0u8; slots.len() * 2 * 4900 * count];
        after.par_chunks_mut(4900 * count).enumerate().for_each(|(i, rows)| {
            let (slot, sign) = (i / 2, i % 2);
            let (c, axis) = slots[slot];
            for (pair, row) in rows.chunks_mut(count).enumerate() {
                for (k, x) in row.iter_mut().enumerate() {
                    *x = dist[c][axis][sign][to[slot][0][k][pair / 70] as usize * 70 + to[slot][1][k][pair % 70] as usize];
                }
            }
        });
        // The scores are added up in bytes.
        let most = |slot: usize| (0..2).map(|sign| *dist[slots[slot].0][slots[slot].1][sign].iter().max().unwrap() as usize).max().unwrap();
        assert!((0..slots.len()).map(most).sum::<usize>() < FAR as usize);
        Short { ring, slots: slots.len(), to, after }
    }

    fn ranks(&self, n: &Node) -> Ranks {
        let mut out = [0u8; 6 * WHOLE];
        for i in 0..self.slots {
            let (c, axis) = (i / 3, i % 3);
            let rank = |x: u32| self.ring.rank8[(x >> (8 * axis) & 255) as usize];
            (out[2 * i], out[2 * i + 1]) = (rank(n[2 * c]), rank(n[2 * c + 1]));
        }
        out
    }

    #[inline(always)]
    fn next(&self, n: &Ranks, k: usize) -> Ranks {
        let mut out = *n;
        for i in 0..self.slots {
            out[2 * i] = self.to[i][0][k][n[2 * i] as usize];
            out[2 * i + 1] = self.to[i][1][k][n[2 * i + 1] as usize];
        }
        out
    }

    /// How many centres of the row do not agree with the one under them
    /// (or are not its opposite, where that is the aim): an even number in
    /// every slot.
    fn apart(&self, n: &Ranks, signs: [usize; 3]) -> u32 {
        let pattern = |r: u8| self.ring.unrank8[r as usize];
        (0..self.slots).map(|i| (pattern(n[2 * i]) ^ pattern(n[2 * i + 1]) ^ if signs[i % 3] == 1 { 255 } else { 0 }).count_ones()).sum()
    }
}

/// One search for all columns of a short row at once, with one choice of
/// signs: a beam search (as `search::beam`, which see) that is taken a
/// level at a time, so that several can run side by side, and whose states
/// can be looked at on the way.
struct Lane<'a> {
    short: &'a Short<'a>,
    choice: usize,
    signs: [usize; 3],
    /// Per slot: `Short::after` for the sign of its axis.
    after: Vec<&'a [u8]>,
    /// The states of the length reached, each with the move that led to it.
    level: Vec<(Ranks, u8)>,
    /// Per length: each state's parent in the level before, and that move.
    trail: Vec<Vec<(u32, u8)>>,
    /// The score every state had on the last level, if they all had the
    /// same one, and for how many levels running.
    flat: (u8, usize),
    over: bool,
}

impl<'a> Lane<'a> {
    fn new(short: &'a Short<'a>, choice: usize, signs: [usize; 3], from: Ranks) -> Lane<'a> {
        let rows = 4900 * short.ring.moves.len();
        let after = (0..short.slots).map(|i| &short.after[(2 * i + signs[i % 3]) * rows..][..rows]).collect();
        Lane { short, choice, signs, after, level: vec![(from, FAR)], trail: Vec::new(), flat: (FAR, 0), over: false }
    }

    /// One move further: the `WIDTH` states with the lowest scores, each
    /// once; of those with the last score that fits, a sample. The score
    /// is `Step::halves`: half moves still needed, added over the slots.
    fn advance(&mut self) {
        if self.over {
            self.level.clear();
            return;
        }
        let (short, after) = (self.short, &self.after);
        let (moves, count) = (&short.ring.moves, short.ring.moves.len());
        let level = &self.level;
        let scores: Vec<u8> = level
            .par_chunks(256)
            .flat_map_iter(|chunk| {
                let mut out = vec![0u8; chunk.len() * count];
                for ((n, last), sums) in chunk.iter().zip(out.chunks_mut(count)) {
                    for (s, after) in after.iter().enumerate() {
                        let pair = n[2 * s] as usize * 70 + n[2 * s + 1] as usize;
                        for (sum, x) in sums.iter_mut().zip(&after[pair * count..][..count]) {
                            *sum += x;
                        }
                    }
                    if *last != FAR {
                        for (sum, &m) in sums.iter_mut().zip(moves) {
                            if skip54(*last, m) {
                                *sum = FAR;
                            }
                        }
                    }
                }
                out
            })
            .collect();
        // The score at which the level is full, with room for states reached twice.
        let most = WIDTH + WIDTH / 2;
        let mut tally = [0usize; 256];
        for &x in &scores {
            tally[x as usize] += 1;
        }
        let (mut cut, mut total) = (0, tally[0]);
        while total < most && cut < FAR as usize - 1 {
            cut += 1;
            total += tally[cut];
        }
        // (score, hash, state, parent, move): a state reached twice has the
        // same score and hash both times, so its copies end up side by side.
        let mut next: Vec<(u8, u32, Ranks, u32, u8)> = scores
            .par_chunks(256 * count)
            .enumerate()
            .flat_map_iter(|(chunk, scores)| {
                let fits = (0..scores.len()).filter(|&j| scores[j] as usize <= cut);
                fits.map(move |j| {
                    let (i, k) = (256 * chunk + j / count, j % count);
                    let to = short.next(&level[i].0, k);
                    (scores[j], mixed(&to, 0), to, i as u32, moves[k])
                })
            })
            .collect();
        if next.len() > most {
            next.select_nth_unstable(most);
            next.truncate(most);
        }
        next.par_sort_unstable();
        next.dedup_by(|x, y| x.2 == y.2);
        next.truncate(WIDTH);
        self.flat = match (next.first(), next.last()) {
            (Some(a), Some(b)) if a.0 == b.0 && a.0 == self.flat.0 => (a.0, self.flat.1 + 1),
            (Some(a), Some(b)) if a.0 == b.0 => (a.0, 1),
            _ => (FAR, 0),
        };
        self.over = next.is_empty() || self.flat.1 >= PATIENCE;
        self.trail.push(next.iter().map(|x| (x.3, x.4)).collect());
        self.level = next.iter().map(|x| (x.2, x.4)).collect();
    }

    /// The states of the length reached that are worth trying for an
    /// ending, those with the lowest scores first, with their moves.
    fn starts(&self) -> Vec<Start> {
        let near = (0..self.level.len()).map(|i| (i, self.short.apart(&self.level[i].0, self.signs))).filter(|x| x.1 <= APART).take(NEAR);
        near.map(|(i, apart)| {
            let mut moves = Vec::with_capacity(self.trail.len());
            let mut at = i;
            for step in self.trail.iter().rev() {
                moves.push(step[at].1);
                at = step[at].0 as usize;
            }
            moves.reverse();
            Start { choice: self.choice, signs: self.signs, moves, at: self.level[i].0, apart }
        })
        .collect()
    }
}

/// A cube part of the way through a matching of all columns at once: the
/// choice of signs it was searched with, the moves so far, the state, and
/// how many centres are still apart from the one under them.
#[derive(Clone)]
struct Start {
    choice: usize,
    signs: [usize; 3],
    moves: Vec<u8>,
    at: Ranks,
    apart: u32,
}

/// The last moves of a matching, searched exactly.
///
/// A centre of the row and the one under it are parted by six turns only,
/// the half turns exactly as deep as the row (see the top of this file);
/// every other move takes the two along together. Such a turn exchanges
/// the centres of the row between two opposite faces, in every column and
/// on both axes it cuts at once. The sum over the columns that the beam
/// goes by counts it once for each column as if the others were not there,
/// which is a good guide while much is out of place and no guide at the
/// end: with one pair of centres left to exchange, the turn that does it
/// undoes other columns, every state in the beam is "one turn away", and
/// the beam walks among such states until it runs out of length. Three
/// searches in four ended like that on a row of four centres, and all of
/// them on most rows of five.
///
/// From there the way is short and a different kind of search finds it:
/// moves that part nothing, until one separating turn exchanges, in every
/// slot, either two centres of one kind or the two that are left. What
/// each slot needs for that by itself is a table of 4900 entries, the
/// largest of them bounds the moves still needed, and with that bound
/// every way of a few moves is looked at. The same holds with two pairs
/// left in two slots, which one turn can exchange together; on some cubes
/// no state with one pair left is ever reached and all end that way.
struct Ending<'a> {
    short: &'a Short<'a>,
    /// Places among the ring's moves of those that part nothing, and of the six separating turns.
    whole: Vec<usize>,
    turns: Vec<usize>,
    /// Per separating turn, slot and sign: moves of `whole` until that
    /// turn leaves the two patterns agreeing (or opposite); `FAR` where it
    /// never does.
    left: Vec<Vec<u8>>,
}

impl<'a> Ending<'a> {
    fn new(short: &'a Short<'a>) -> Ending<'a> {
        let ring = short.ring;
        let deep = |k: &usize| ring.moves[*k] as usize / 18;
        let whole: Vec<usize> = (0..ring.moves.len()).filter(|k| deep(k) != ring.row).collect();
        let turns: Vec<usize> = (0..ring.moves.len()).filter(|k| deep(k) == ring.row).collect();
        let wanted: Vec<(usize, usize, usize)> =
            (0..turns.len()).flat_map(|t| (0..short.slots).flat_map(move |i| (0..2).map(move |sign| (t, i, sign)))).collect();
        let left = wanted
            .par_iter()
            .map(|&(t, i, sign)| {
                let to = &short.to[i];
                let mut dist = vec![FAR; 4900];
                let mut level: Vec<u16> = Vec::new();
                for x in 0..70 {
                    for y in 0..70 {
                        let after = [to[0][turns[t]][x], to[1][turns[t]][y]].map(|r| ring.unrank8[r as usize]);
                        if after[0] == after[1] ^ if sign == 1 { 255 } else { 0 } {
                            dist[x * 70 + y] = 0;
                            level.push((x * 70 + y) as u16);
                        }
                    }
                }
                // The moves that part nothing come with their inverses, so
                // the way out from the goals is the way in.
                let mut depth = 0;
                while !level.is_empty() {
                    depth += 1;
                    let mut next = Vec::new();
                    for &s in &level {
                        let (x, y) = (s as usize / 70, s as usize % 70);
                        for &k in &whole {
                            let t = to[0][k][x] as usize * 70 + to[1][k][y] as usize;
                            if dist[t] == FAR {
                                dist[t] = depth;
                                next.push(t as u16);
                            }
                        }
                    }
                    level = next;
                }
                dist
            })
            .collect();
        Ending { short, whole, turns, left }
    }

    /// A matching that continues one of the starts with exactly `depth`
    /// moves and a separating turn, with the choice of signs it belongs to:
    /// of several, the one from the earliest start. A try gives up after
    /// `budget` positions.
    fn search(&self, starts: &[Start], depth: usize, budget: u64) -> Option<(usize, Vec<u8>)> {
        let slots = self.short.slots;
        let tries: Vec<(&Start, usize)> = starts.iter().flat_map(|s| (0..self.turns.len()).map(move |t| (s, t))).collect();
        tries.par_iter().find_map_first(|&(s, t)| {
            let tables: Vec<&[u8]> = (0..slots).map(|i| &self.left[(t * slots + i) * 2 + s.signs[i % 3]][..]).collect();
            let needs = |i: usize| tables[i][s.at[2 * i] as usize * 70 + s.at[2 * i + 1] as usize];
            if (0..slots).any(|i| needs(i) as usize > depth) {
                return None;
            }
            let mut order: Vec<usize> = (0..slots).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(needs(i)));
            let mut run = Run { end: self, tables: &tables, order: &order, path: Vec::new(), seen: 0, budget };
            run.dfs(&s.at, depth, FAR).then(|| {
                let mut moves = s.moves.clone();
                moves.extend(&run.path);
                moves.push(self.short.ring.moves[self.turns[t]]);
                (s.choice, moves)
            })
        })
    }
}

/// One search of an ending: from one start, for one separating turn.
struct Run<'a> {
    end: &'a Ending<'a>,
    /// Per slot: the table of that turn for the start's sign.
    tables: &'a [&'a [u8]],
    /// The slots, those that need most moves first.
    order: &'a [usize],
    path: Vec<u8>,
    seen: u64,
    budget: u64,
}

impl Run<'_> {
    /// Whether exactly `left` moves that part nothing lead to where the
    /// turn finishes; they are left in `path`. The caller has seen that
    /// no table asks for more than `left` moves here.
    fn dfs(&mut self, at: &Ranks, left: usize, last: u8) -> bool {
        if left == 0 {
            return true;
        }
        self.seen += 1;
        if self.seen > self.budget {
            return false;
        }
        let short = self.end.short;
        'moves: for &k in &self.end.whole {
            let m = short.ring.moves[k];
            if last != FAR && skip54(last, m) {
                continue;
            }
            // The tables first say no for the slots out of place, which
            // `order` puts first: most moves are turned away by one of them.
            let mut next = *at;
            for &i in self.order {
                let to = &short.to[i];
                (next[2 * i], next[2 * i + 1]) = (to[0][k][at[2 * i] as usize], to[1][k][at[2 * i + 1] as usize]);
                if self.tables[i][next[2 * i] as usize * 70 + next[2 * i + 1] as usize] as usize >= left {
                    continue 'moves;
                }
            }
            self.path.push(m);
            if self.dfs(&next, left - 1, m) {
                return true;
            }
            self.path.pop();
        }
        false
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

/// A name for a table from what it is made of (FNV-1a over the moves, their
/// costs and the orbits taken as opposites).
fn fingerprint(to: &[Vec<u32>], costs: &[usize], flips: &[bool]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |x: u32| {
        for b in x.to_le_bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        }
    };
    for (column, &cost) in to.iter().zip(costs) {
        eat(cost as u32);
        column.iter().for_each(|&x| eat(x));
    }
    flips.iter().for_each(|&x| eat(x as u32 + 7));
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cube whose centres are staged and nothing more: face turns and
    /// half turns of any depth from the solved cube.
    fn staged(g: &Geometry, mut seed: u64) -> Vec<u8> {
        let moves: Vec<usize> = (0..g.move_count()).filter(|m| m / 18 == 0 || m % 3 == 1).collect();
        let mut f = g.solved();
        for _ in 0..300 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            g.apply(&mut f, moves[(seed >> 33) as usize % moves.len()]);
        }
        f
    }

    /// The last row of an even cube (four centres) and of an odd one
    /// (three), all columns in one search with its ending. (A row of five
    /// is searched the same way and falls back on one cube in ten.)
    #[test]
    fn a_short_row_is_matched_with_all_its_columns_at_once() {
        for (n, row) in [(8, 1), (7, 1)] {
            let g = Geometry::new(n);
            let ring = Ring::new(&g, row);
            let mut f = staged(&g, 7 + n as u64);
            let root = ring.node(&f).unwrap();
            let every: Vec<usize> = (0..ring.moves.len()).collect();
            let dist: Vec<[[Vec<u8>; 2]; 3]> = (0..ring.len())
                .map(|k| {
                    [0, 1, 2].map(|axis| {
                        [false, true].map(|opposite| {
                            pair_table(&ring.moves, &every, &ring.perms[2 * k], &ring.perms[2 * k + 1], &ring.rank8, &ring.unrank8, axis, opposite)
                        })
                    })
                })
                .collect();
            let choices: Vec<(u32, [usize; 3])> = (0..8usize).map(|bits| (0, [bits & 1, bits >> 1 & 1, bits >> 2])).collect();
            let (_, moves, ending) = ring.whole(&root, &dist, &choices).expect("a matching");
            assert!(ending <= EASY + 1 && moves.len() < 45);
            g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            assert!(ring.matched(&f));
        }
    }
}
