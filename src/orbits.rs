//! The centres of a cube of any size, orbit by orbit.
//!
//! The centre at row `r`, column `c` of a face belongs to an orbit of 24:
//! its four turned copies on each of the six faces. There are three kinds.
//! An x centre (`r == c`) of any cube moves exactly like the centres of a
//! 4x4x4, and a t centre (on the middle column of an odd cube) like the t
//! centres of a 5x5x5, so their positions are numbered the same way here
//! and the tables of those cubes apply unchanged. Oblique centres come in
//! mirror pairs: `(r, c)` and its mate across the middle of the row,
//! `(r, n - 1 - c)`. Turns that reach no deeper than row `r` move the two
//! mates together, and move them as the t centre between them would move;
//! oblique orbits are numbered like t centres too, so that a centre and its
//! mate have the same number.

use crate::c5::tables5;
use crate::cube::tables as cube_tables;
use crate::cuben::Geometry;
use crate::sym::{conjugators, destinations, MaskMap};

/// Faces in the order the positions are numbered: U D F B R L.
const FACE_ORDER: [usize; 6] = [0, 3, 2, 5, 1, 4];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    X,
    T,
    Oblique,
}

pub struct CentreOrbit {
    pub kind: Kind,
    /// Row and column (from 0) of the cell that names the orbit.
    pub cell: (usize, usize),
    /// Facelet of each position. Positions 0..8 are on U and D, 8..16 on F
    /// and B, 16..24 on R and L.
    pub facelet: [usize; 24],
    /// `src[m][i]`: the position whose piece move `m` of the cube brings to position `i`.
    pub src: Vec<[u8; 24]>,
}

impl CentreOrbit {
    /// Colours of the orbit's 24 centres.
    pub fn colours(&self, facelets: &[u8]) -> [u8; 24] {
        let mut out = [0u8; 24];
        for i in 0..24 {
            out[i] = facelets[self.facelet[i]];
        }
        out
    }
}

/// Every centre orbit of the cube, from the outer ring inwards.
pub fn centre_orbits(g: &Geometry) -> Vec<CentreOrbit> {
    let n = g.n;
    let mut out = Vec::new();
    for r in 1..n - 1 {
        for c in r..n.saturating_sub(1 + r) {
            let kind = if r == c {
                Kind::X
            } else if n % 2 == 1 && c == n / 2 {
                Kind::T
            } else {
                Kind::Oblique
            };
            out.push(orbit(g, kind, (r, c)));
        }
    }
    out
}

fn orbit(g: &Geometry, kind: Kind, cell: (usize, usize)) -> CentreOrbit {
    let n = g.n;
    // Raw numbering: face by face, the cell and its three quarter turns.
    let mut facelet = [0usize; 24];
    for (k, &face) in FACE_ORDER.iter().enumerate() {
        let (mut r, mut c) = cell;
        for j in 0..4 {
            facelet[k * 4 + j] = face * n * n + r * n + c;
            (r, c) = (c, n - 1 - r);
        }
    }
    let src_of = |facelet: &[usize; 24]| -> Vec<[u8; 24]> {
        (0..g.move_count())
            .map(|m| {
                let src = g.move_src(m);
                let mut row = [0u8; 24];
                for i in 0..24 {
                    let from = src[facelet[i]] as usize;
                    row[i] = facelet.iter().position(|&f| f == from).expect("an orbit is closed under moves") as u8;
                }
                row
            })
            .collect()
    };
    // Renumber the orbit like the pieces it moves like.
    let reference: &[[u8; 24]] = match kind {
        Kind::X => &cube_tables().ct_src,
        Kind::T | Kind::Oblique => &tables5().t_src,
    };
    let raw = destinations(&src_of(&facelet));
    let like = destinations(reference);
    // A turn of `layers` layers reaches the orbit's nearest slice or it does not.
    // An oblique is compared only under the turns that do not reach its second slice.
    let deepest = if kind == Kind::Oblique { cell.1.min(n - 1 - cell.1) } else { n };
    let pairs: Vec<(usize, usize)> = (0..g.move_count())
        .filter(|m| m / 18 + 1 <= deepest)
        .map(|m| {
            let (face_power, layers) = (m % 18, m / 18 + 1);
            (m, face_power + if layers > cell.0 { 18 } else { 0 })
        })
        .collect();
    // The first raw position is the naming cell on U, in the row nearest B,
    // like the first t centre. Pinning it there gives mates the same numbers.
    let fixed: &[(usize, usize)] = if kind == Kind::Oblique { &[(0, 0)] } else { &[] };
    let found = conjugators(24, &pairs, &|m, x| raw[m][x] as usize, &|m, x| like[m][x] as usize, fixed);
    let p = found
        .into_iter()
        .find(|p| {
            let map = MaskMap::new(p);
            map.apply(0x0000FF) == 0x0000FF && map.apply(0x00FF00) == 0x00FF00
        })
        .expect("the orbit moves like its model");
    let mut renumbered = [0usize; 24];
    for i in 0..24 {
        renumbered[p[i] as usize] = facelet[i];
    }
    facelet = renumbered;
    let src = src_of(&facelet);
    CentreOrbit { kind, cell, facelet, src }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_counts() {
        let kinds = |n: usize| -> Vec<Kind> { centre_orbits(&Geometry::new(n)).iter().map(|o| o.kind).collect() };
        assert_eq!(kinds(4), vec![Kind::X]);
        assert_eq!(kinds(5), vec![Kind::X, Kind::T]);
        assert_eq!(kinds(6), vec![Kind::X, Kind::Oblique, Kind::Oblique, Kind::X]);
        assert_eq!(kinds(7).iter().filter(|&&k| k == Kind::Oblique).count(), 2);
        assert_eq!(kinds(7).len(), 6);
    }

    #[test]
    fn x_and_t_orbits_move_like_their_models() {
        for n in [5usize, 6, 7] {
            let g = Geometry::new(n);
            for o in centre_orbits(&g) {
                let reference: &[[u8; 24]] = match o.kind {
                    Kind::X => &cube_tables().ct_src,
                    Kind::T => &tables5().t_src,
                    Kind::Oblique => continue,
                };
                for m in 0..g.move_count() {
                    let like = m % 18 + if m / 18 + 1 > o.cell.0 { 18 } else { 0 };
                    assert_eq!(o.src[m], reference[like], "size {} cell {:?} move {}", n, o.cell, m);
                }
            }
        }
    }

    #[test]
    fn oblique_mates_share_numbers() {
        for n in [6usize, 7] {
            let g = Geometry::new(n);
            let obliques: Vec<CentreOrbit> = centre_orbits(&g).into_iter().filter(|o| o.kind == Kind::Oblique).collect();
            assert_eq!(obliques.len(), 2);
            for i in 0..24 {
                let (a, b) = (obliques[0].facelet[i], obliques[1].facelet[i]);
                let (ra, ca, rb, cb) = (a / n % n, a % n, b / n % n, b % n);
                assert_eq!(a / (n * n), b / (n * n), "same face");
                // Mirror images across the middle of their row or column.
                assert!((ra == rb && ca + cb == n - 1) || (ca == cb && ra + rb == n - 1), "size {} position {}", n, i);
            }
        }
    }

    #[test]
    fn the_5x5x5_orbits_are_the_ones_of_its_piece_model() {
        // Same facelets as the piece model reads, so colourings agree position by position.
        let g = Geometry::new(5);
        let mut f = g.solved();
        g.apply_seq(&mut f, &g.parse_moves("Rw U2 F' Uw L2 D Bw' R Fw Dw' L").unwrap());
        let cube = crate::c5::Cube5::from_facelets(&f).unwrap();
        let orbits = centre_orbits(&g);
        assert_eq!(orbits[0].colours(&f), cube.c4.ct);
        assert_eq!(orbits[1].colours(&f), cube.t);
    }
}
