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
//! Two ways are tried, the shorter one first:
//!
//! - all columns at once, a beam search whose score adds, over the columns
//!   and the three axes, the exact number of half moves until the patterns
//!   of a centre and the one under it agree, each move's cost shared
//!   between the axes;
//! - column by column from the middle of the row, the columns done before
//!   followed and brought back. Longer, but it gets through on some cubes
//!   where the first is stuck.
//!
//! Both work on rows of up to five centres (cubes up to 9x9x9) and on
//! most but not all cubes; on rows of six neither found anything. The
//! caller then has to bring every centre home the long way (`axes`).

use crate::cuben::Geometry;
use crate::search::{beam_from, Problem};
use crate::tables::BitPerm;

const FAR: u8 = 255;
/// Centres followed at once: twice the length of a row.
const MAX: usize = 48;
/// The longest row the searches here get through: on rows of six centres
/// neither of them found a matching on any cube tried.
const LONGEST: usize = 5;

type Node = [u32; MAX];

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
        Ring { row: a, level, moves, cells, perms, rank8, unrank8 }
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
    /// The eight choices are tried from the most promising one.
    ///
    /// With the moves, whether they were found the first way.
    pub fn solve(&self, g: &Geometry, f: &[u8]) -> Result<(Vec<u8>, bool), String> {
        if self.level.len() > LONGEST {
            return Err(format!("the centres of layer {} are a row of {}, too long to match", self.row + 1, self.level.len()));
        }
        let root = self.node(f)?;
        let deepest = *self.level.iter().max().unwrap();
        let all: Vec<usize> = (0..self.level.len()).collect();
        let table = |k: usize, axis: usize| {
            [false, true].map(|opposite| {
                pair_table(&self.moves, &self.perms[2 * k], &self.perms[2 * k + 1], &self.rank8, &self.unrank8, axis, opposite)
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
        let search = |step: &Step, from: &Node, width: usize| {
            let score = |n: &Node| step.halves(n);
            beam_from(step, std::slice::from_ref(from), score, width, 1, 70, 0, &|_| true).into_iter().next()
        };
        for (number, &(_, signs)) in choices.iter().enumerate() {
            if let Some(moves) = search(&step(all.clone(), signs), &root, 30_000) {
                if trace {
                    eprintln!("    row {}: all columns at once, choice {}, {} moves", self.row, number, moves.len());
                }
                return Ok((moves, true));
            }
        }
        for (number, &(_, signs)) in choices.iter().enumerate() {
            let mut f = f.to_vec();
            let mut out: Vec<u8> = Vec::new();
            let done = (self.row + 1..=deepest).rev().all(|level| {
                let columns: Vec<usize> = all.iter().copied().filter(|&k| self.level[k] >= level).collect();
                let Ok(from) = self.node(&f) else { return false };
                match search(&step(columns, signs), &from, 30_000) {
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
                    eprintln!("    row {}: column by column, choice {}, {} moves", self.row, number, out.len());
                }
                return Ok((out, false));
            }
        }
        Err(format!("no matching found for the centres of layer {}", self.row + 1))
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
                        let to = perm.apply(i as u8, unrank8[r] << shift) >> shift;
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
