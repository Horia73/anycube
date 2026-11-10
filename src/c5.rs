//! 5x5x5 cube model.
//!
//! With turns of one or two layers the middle slices never move, so a 5x5x5
//! is three things that move together:
//! - a 4x4x4 (corners, wings and the corner-ish "x" centres), which is the
//!   5x5x5 with its middle rows and columns left out;
//! - the 12 middle edges, which move like 3x3x3 edges;
//! - the 24 edge-ish "t" centres.
//! The 36 moves have the numbering of `cube.rs`.

use crate::c3::C3;
use crate::cube::Cube;
use crate::cuben::Geometry;
use std::sync::OnceLock;

/// Faces in the order the centre positions are numbered: U D F B R L,
/// four positions each, as for the x centres.
const FACE_ORDER: [usize; 6] = [0, 3, 2, 5, 1, 4];
/// The t centres of a face, clockwise from the top one.
const T_CELLS: [(usize, usize); 4] = [(1, 2), (2, 3), (3, 2), (2, 1)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cube5 {
    /// Corners, wings and x centres.
    pub c4: Cube,
    /// Middle edges; the corner part is unused.
    pub mid: C3,
    /// Colours of the t centres.
    pub t: [u8; 24],
}

pub struct Tables5 {
    pub geometry: Geometry,
    /// `t_src[m][i]`: the t-centre position whose piece move `m` brings to position `i`.
    pub t_src: Vec<[u8; 24]>,
    /// Facelet of each t-centre position.
    t_facelet: [usize; 24],
    /// The 18 face turns as they act on the middle edges.
    mid_moves: [C3; 18],
}

pub fn tables5() -> &'static Tables5 {
    static T: OnceLock<Tables5> = OnceLock::new();
    T.get_or_init(|| {
        let geometry = Geometry::new(5);
        let mut t_facelet = [0usize; 24];
        for (k, &face) in FACE_ORDER.iter().enumerate() {
            for (j, &(r, c)) in T_CELLS.iter().enumerate() {
                t_facelet[k * 4 + j] = face * 25 + r * 5 + c;
            }
        }
        let mut t_src = Vec::new();
        for m in 0..36 {
            let src = geometry.move_src(m);
            let mut row = [0u8; 24];
            for i in 0..24 {
                let from = src[t_facelet[i]] as usize;
                row[i] = t_facelet.iter().position(|&f| f == from).expect("t centres stay t centres") as u8;
            }
            t_src.push(row);
        }
        let mut mid_moves = [C3::identity(); 18];
        for m in 0..18 {
            mid_moves[m] = C3::from_cube(&Cube::from_moves(&[m as u8])).unwrap();
        }
        Tables5 { geometry, t_src, t_facelet, mid_moves }
    })
}

impl Cube5 {
    pub fn solved() -> Cube5 {
        let mut t = [0u8; 24];
        for (k, &face) in FACE_ORDER.iter().enumerate() {
            for j in 0..4 {
                t[k * 4 + j] = face as u8;
            }
        }
        Cube5 { c4: Cube::solved(), mid: C3::identity(), t }
    }

    /// Reads 150 facelets (colours 0..6 in URFDLB order).
    pub fn from_facelets(f: &[u8]) -> Result<Cube5, String> {
        if f.len() != 150 {
            return Err(format!("expected 150 facelets, got {}", f.len()));
        }
        let pick = |rows: &[usize]| -> String {
            let n = rows.len();
            (0..6 * n * n)
                .map(|i| b"URFDLB"[f[i / (n * n) * 25 + rows[i / n % n] * 5 + rows[i % n]] as usize] as char)
                .collect()
        };
        let c4 = Cube::from_facelets(&pick(&[0, 1, 3, 4]))?;
        let mid = C3::from_cube(&Cube::from_small_facelets(&pick(&[0, 2, 4]))?)
            .ok_or("the middle edges do not form a 3x3x3")?;
        let tb = tables5();
        let mut t = [0u8; 24];
        for i in 0..24 {
            t[i] = f[tb.t_facelet[i]];
        }
        let mut count = [0; 6];
        for &c in &t {
            count[c as usize] += 1;
        }
        if count != [4; 6] {
            return Err("each colour must appear on exactly 4 t centres".into());
        }
        Ok(Cube5 { c4, mid, t })
    }

    pub fn apply(&mut self, m: u8) {
        let tb = tables5();
        self.c4.apply(m);
        // A wide turn carries the same middle edges as the face turn.
        self.mid = self.mid.mul(&tb.mid_moves[m as usize % 18]);
        let old = self.t;
        let src = &tb.t_src[m as usize];
        for i in 0..24 {
            self.t[i] = old[src[i] as usize];
        }
    }

    pub fn apply_seq(&mut self, seq: &[u8]) {
        for &m in seq {
            self.apply(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agrees_with_the_sticker_model() {
        let g = &tables5().geometry;
        let mut f = g.solved();
        let mut cube = Cube5::solved();
        assert_eq!(Cube5::from_facelets(&f).unwrap(), cube);
        let mut seed = 5u64;
        for _ in 0..300 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let m = (seed >> 33) as usize % 36;
            g.apply(&mut f, m);
            cube.apply(m as u8);
            let mut read = Cube5::from_facelets(&f).unwrap();
            // The corner part of the middle-edge cube is not tracked.
            read.mid.cp = cube.mid.cp;
            read.mid.co = cube.mid.co;
            assert_eq!(read, cube);
        }
    }
}
