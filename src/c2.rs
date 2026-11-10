//! 2x2x2: optimal solver.
//!
//! With the DBL corner held still, the cube has 7! * 3^6 = 3,674,160 states
//! and only U, R and F turn. A table of the exact distance of every state
//! gives an optimal solution by walking downhill.

use crate::c3::{perm_rank, perm_unrank};
use crate::cube::{tables as cube_tables, Cube};
use crate::tables::{load_or_gen, table_dir, Bytes};

const N_PERM: usize = 5040;
const N_TWIST: usize = 729;
/// States at each distance, in face turns.
const EXPECTED: [u32; 12] = [1, 9, 54, 321, 1847, 9992, 50136, 227536, 870072, 1887748, 623800, 2644];
/// The seven corners that move, in coordinate order.
const FREE: [usize; 7] = [0, 1, 2, 3, 4, 5, 7];

pub struct T2 {
    dist: Bytes,
    perm_mv: Vec<[u16; 9]>,
    twist_mv: Vec<[u16; 9]>,
}

fn encode(cp: &[u8; 8], co: &[u8; 8]) -> (usize, usize) {
    let mut p = [0u8; 7];
    for (i, &c) in FREE.iter().enumerate() {
        p[i] = if cp[c] == 7 { 6 } else { cp[c] };
    }
    let twist = (0..6).fold(0, |a, i| a * 3 + co[i] as usize);
    (perm_rank(&p), twist)
}

fn decode(perm: usize, twist: usize) -> ([u8; 8], [u8; 8]) {
    let mut p = [0u8; 7];
    perm_unrank(perm, &mut p);
    let mut cp = [6u8; 8];
    for (i, &c) in FREE.iter().enumerate() {
        cp[c] = if p[i] == 6 { 7 } else { p[i] };
    }
    let mut co = [0u8; 8];
    let (mut x, mut sum) = (twist, 0);
    for i in (0..6).rev() {
        co[i] = (x % 3) as u8;
        sum += x % 3;
        x /= 3;
    }
    co[7] = ((12 - sum) % 3) as u8;
    (cp, co)
}

impl T2 {
    pub fn load() -> T2 {
        let ct = cube_tables();
        let mut perm_mv = vec![[0u16; 9]; N_PERM];
        let mut twist_mv = vec![[0u16; 9]; N_TWIST];
        let apply = |cp: &[u8; 8], co: &[u8; 8], m: usize| {
            let (mcp, mco) = (&ct.c_cp[m], &ct.c_co[m]);
            assert_eq!(mcp[6], 6, "U, R and F leave the DBL corner alone");
            let mut ncp = [0u8; 8];
            let mut nco = [0u8; 8];
            for c in 0..8 {
                ncp[c] = cp[mcp[c] as usize];
                nco[c] = (co[mcp[c] as usize] + mco[c]) % 3;
            }
            encode(&ncp, &nco)
        };
        for p in 0..N_PERM {
            let (cp, co) = decode(p, 0);
            for m in 0..9 {
                perm_mv[p][m] = apply(&cp, &co, m).0 as u16;
            }
        }
        for t in 0..N_TWIST {
            let (cp, co) = decode(0, t);
            for m in 0..9 {
                twist_mv[t][m] = apply(&cp, &co, m).1 as u16;
            }
        }
        let dist = load_or_gen(&table_dir().join("cube222.bin"), N_PERM * N_TWIST, || {
            let mut dist = vec![255u8; N_PERM * N_TWIST];
            dist[0] = 0;
            let mut frontier = vec![0u32];
            let mut counts = vec![1u32];
            let mut depth = 0u8;
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for &x in &frontier {
                    let (p, t) = (x as usize / N_TWIST, x as usize % N_TWIST);
                    for m in 0..9 {
                        let y = perm_mv[p][m] as usize * N_TWIST + twist_mv[t][m] as usize;
                        if dist[y] == 255 {
                            dist[y] = depth + 1;
                            next.push(y as u32);
                        }
                    }
                }
                if !next.is_empty() {
                    counts.push(next.len() as u32);
                }
                frontier = next;
                depth += 1;
            }
            assert_eq!(counts, EXPECTED, "2x2x2 distance distribution differs from the known one");
            dist
        });
        T2 { dist, perm_mv, twist_mv }
    }

    /// An optimal solution in face turns of U, R and F. The cube may be in any
    /// orientation; it ends solved in the orientation its DBL corner dictates.
    pub fn solve(&self, cube: &Cube) -> Vec<u8> {
        // Turn the whole cube so that the DBL corner is home.
        let home = cube_tables()
            .rots
            .iter()
            .map(|r| cube.after_rotation(&r.state))
            .find(|c| c.cp[6] == 6 && c.co[6] == 0)
            .expect("some orientation puts the DBL corner home");
        let (mut p, mut t) = encode(&home.cp, &home.co);
        let mut out = Vec::new();
        let mut d = self.dist[p * N_TWIST + t];
        while d > 0 {
            for m in 0..9 {
                let (a, b) = (self.perm_mv[p][m] as usize, self.twist_mv[t][m] as usize);
                if self.dist[a * N_TWIST + b] == d - 1 {
                    (p, t, d) = (a, b, d - 1);
                    out.push(m as u8);
                    break;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::parse_moves;

    #[test]
    fn solves_optimally() {
        let t = T2::load();
        for (scr, len) in [("", 0), ("R", 1), ("L", 1), ("R U R' U'", 4), ("D B2 L' U F R2 D' B", 8)] {
            let cube = Cube::from_moves(&parse_moves(scr).unwrap());
            let sol = t.solve(&cube);
            assert!(sol.len() <= len, "{}", scr);
            let mut c = cube;
            c.apply_seq(&sol);
            let ok = cube_tables().rots.iter().any(|r| r.state.cp == c.cp && r.state.co == c.co);
            assert!(ok, "{}", scr);
        }
    }
}
