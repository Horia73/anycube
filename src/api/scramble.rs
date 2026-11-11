//! Scrambles from a random state: a cube picked at random from all the
//! cubes there are, each equally likely, and the moves that lead to it.
//!
//! That is what competitions use for the small cubes, and it is only
//! possible where a solver is at hand: the scramble is a solution of the
//! random cube played backwards. Here that is the 2x2x2 and the 3x3x3.

use crate::api::solve::{t2, t3};
use crate::c3::C3;
use crate::cube::{inv_seq, Cube};
use crate::cuben::Geometry;

/// SplitMix64: enough for shuffling, and the same on every machine.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// A number below `n`, every one equally likely.
    fn below(&mut self, n: u64) -> usize {
        // Reject the few values at the top that would make some numbers likelier.
        let limit = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next();
            if x < limit {
                return (x % n) as usize;
            }
        }
    }

    fn shuffle(&mut self, v: &mut [u8]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i as u64 + 1));
        }
    }
}

/// Whether a permutation is odd.
fn odd(p: &[u8]) -> bool {
    let mut seen = [false; 12];
    let mut odd = false;
    for i in 0..p.len() {
        let (mut j, mut len) = (i, 0);
        while !seen[j] {
            seen[j] = true;
            j = p[j] as usize;
            len += 1;
        }
        odd ^= len > 0 && len % 2 == 0;
    }
    odd
}

/// A scramble that leads to a 2x2x2 or 3x3x3 picked at random from all
/// positions; `None` for other sizes. The same `seed` gives the same scramble.
pub fn random_state(size: usize, seed: u64) -> Option<String> {
    let mut rnd = Random(seed);
    let g = Geometry::new(size);
    let solution = loop {
        let solution = match size {
            2 => {
                // The corner at D, B and L (number 6) stays; the other seven
                // go anywhere, six twist freely and the last makes the sum right.
                let mut pieces = [0u8, 1, 2, 3, 4, 5, 7];
                rnd.shuffle(&mut pieces);
                let mut cube = Cube::solved();
                let mut twist = 0;
                for (k, &place) in [0usize, 1, 2, 3, 4, 5, 7].iter().enumerate() {
                    cube.cp[place] = pieces[k];
                    cube.co[place] = if place == 7 { (3 - twist % 3) % 3 } else { rnd.below(3) as u8 };
                    twist += cube.co[place];
                }
                t2().solve(&cube)
            }
            3 => {
                let mut c = C3::identity();
                rnd.shuffle(&mut c.cp);
                rnd.shuffle(&mut c.ep);
                // Corners and edges are both an even or both an odd permutation.
                if odd(&c.cp) != odd(&c.ep) {
                    c.ep.swap(0, 1);
                }
                let mut twist = 0;
                for i in 0..8 {
                    c.co[i] = if i == 7 { (3 - twist % 3) % 3 } else { rnd.below(3) as u8 };
                    twist += c.co[i];
                }
                let mut flip = 0;
                for i in 0..12 {
                    c.eo[i] = if i == 11 { flip % 2 } else { rnd.below(2) as u8 };
                    flip += c.eo[i];
                }
                // A scramble need not be the shortest way there: a quick search, about 20 moves.
                t3().solve(&c, 2_000, None)
            }
            _ => return None,
        };
        // A cube a few moves from solved is no scramble: draw again, as competitions do.
        if solution.len() >= 4 {
            break solution;
        }
    };
    let names: Vec<String> = inv_seq(&solution).iter().map(|&m| g.move_name(m as usize)).collect();
    Some(names.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::check::{check, validate, Input};
    use std::collections::HashSet;

    #[test]
    fn random_states_are_cubes_and_differ() {
        // Fewer of the 3x3x3: each is a search, slow in an unoptimised build.
        for (size, count) in [(2, 40u64), (3, 8)] {
            let mut seen = HashSet::new();
            for seed in 0..count {
                let scramble = random_state(size, seed * 7919 + 1).unwrap();
                let moves = scramble.split(' ').count();
                assert!(moves >= 4 && moves <= if size == 2 { 11 } else { 24 }, "{}", scramble);
                // No move undoes or merges with the one before it.
                let faces: Vec<char> = scramble.split(' ').map(|m| m.chars().next().unwrap()).collect();
                assert!(faces.windows(2).all(|w| w[0] != w[1]), "{}", scramble);
                let cube = check(size, Input::Moves(&scramble), 3).unwrap();
                validate(size, &cube.facelets).unwrap();
                seen.insert(cube.facelets);
            }
            assert_eq!(seen.len() as u64, count, "as many cubes as seeds");
        }
        assert_eq!(random_state(3, 5), random_state(3, 5));
        assert_eq!(random_state(4, 5), None);
    }

    #[test]
    fn every_corner_and_twist_turns_up() {
        // Uniform over the states: each corner piece is seen in each place, twisted each way.
        let g = Geometry::new(2);
        let mut seen = HashSet::new();
        for seed in 0..600u64 {
            let scramble = random_state(2, seed).unwrap();
            let mut f = g.solved();
            g.apply_seq(&mut f, &g.parse_moves(&scramble).unwrap());
            // The sticker on U at the corner U, R, F, and the one at U, B, L.
            seen.insert((0, f[3]));
            seen.insert((1, f[0]));
        }
        assert_eq!(seen.len(), 12, "six colours at each of two stickers");
    }

    #[test]
    fn parity() {
        assert!(!odd(&[0, 1, 2, 3]));
        assert!(odd(&[1, 0, 2, 3]));
        assert!(!odd(&[1, 2, 0, 3]));
        assert!(odd(&[1, 2, 3, 0]));
    }
}
