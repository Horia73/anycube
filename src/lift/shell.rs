//! Remove the middle row and column of each 5x5x5 face to obtain a 4x4x4.
//! A wide turn of that shell can be played from either end of its axis if
//! the remaining shell word is reoriented. The middle band of the 5x5x5
//! distinguishes the two choices.

use super::{inv_move, Cube, Cube5, Params, Solver};
use crate::cube::tables;
use std::sync::OnceLock;

#[derive(Clone, Copy)]
struct Turn {
    physical: u8,
    hold: usize,
}

type Transitions = Vec<[Vec<Turn>; 36]>;

fn transitions() -> &'static Transitions {
    static ALL: OnceLock<Transitions> = OnceLock::new();
    ALL.get_or_init(|| {
        let t = tables();
        // Label every centre separately so equality means the same action
        // on the shell, even when a scrambled face has repeated colours.
        let mut marked = Cube::solved();
        marked.ct = std::array::from_fn(|i| i as u8);
        let rotate = |mut state: Cube, r: usize| {
            state.apply_seq(&t.rots[r].seq);
            state
        };
        (0..t.rots.len())
            .map(|hold| {
                std::array::from_fn(|m| {
                    let physical = t.relabel[t.rot_inv[hold]][m];
                    let mut choices = vec![Turn { physical, hold }];
                    if m >= 18 {
                        let opposite = 18 + ((physical / 3 + 3) % 6) * 3 + physical % 3;
                        let mut actual = rotate(marked, hold);
                        actual.apply(opposite);
                        let mut reference = marked;
                        reference.apply(m as u8);
                        let next = (0..24)
                            .find(|&r| rotate(reference, r) == actual)
                            .expect("opposite wide turns on a 4x4x4 differ by a rotation");
                        choices.push(Turn {
                            physical: opposite,
                            hold: next,
                        });
                    }
                    choices
                })
            })
            .collect()
    })
}

fn lift_word(start: &Cube5, word: &[u8]) -> Option<Vec<u8>> {
    let t = tables();
    let choices = transitions();
    let mut shell = start.c4;
    shell.apply_seq(word);
    let end = t.rots.iter().position(|r| shell == r.state)?;
    // Only keep holds that can put the corners back in the 5x5x5's fixed frame.
    let mut reachable = vec![[false; 24]; word.len() + 1];
    reachable[word.len()][t.rot_inv[end]] = true;
    for i in (0..word.len()).rev() {
        for hold in 0..24 {
            reachable[i][hold] = choices[hold][word[i] as usize].iter().any(|t| reachable[i + 1][t.hold]);
        }
    }
    fn visit(
        c: &Cube5,
        word: &[u8],
        choices: &Transitions,
        reachable: &[[bool; 24]],
        hold: usize,
        path: &mut Vec<u8>,
        budget: &mut usize,
    ) -> bool {
        let i = path.len();
        if !reachable[i][hold] || *budget == 0 {
            return false;
        }
        if i == word.len() {
            return *c == Cube5::solved();
        }
        *budget -= 1;
        for next in &choices[hold][word[i] as usize] {
            if reachable[i + 1][next.hold] {
                let mut moved = *c;
                moved.apply(next.physical);
                path.push(next.physical);
                if visit(&moved, word, choices, reachable, next.hold, path, budget) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
    let mut path = Vec::new();
    let mut budget = 50_000;
    visit(start, word, choices, &reachable, 0, &mut path, &mut budget).then_some(path)
}

pub(super) fn improve(solver: &Solver, path: &[u8], bound: usize) -> Option<Vec<u8>> {
    let last = path.iter().rposition(|&m| m >= 18)?;
    let mut at = Cube5::solved();
    let mut limit = bound;
    let mut best = None;
    let params = Params::fast();
    for cut in (last.saturating_sub(15)..path.len()).rev() {
        at.apply(inv_move(path[cut]));
        if cut > last || !(last - cut).is_multiple_of(2) || cut >= limit {
            continue;
        }
        let mut proposals = Vec::new();
        if let Ok(winner) = solver.solve_with_alternatives(&at.c4, &params, Some(&mut proposals)) {
            proposals.push(winner.moves);
        }
        proposals.retain(|word| cut + word.len() < limit);
        proposals.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        proposals.dedup();
        if let Some(tail) = proposals.iter().find_map(|word| lift_word(&at, word)) {
            let mut found = path[..cut].to_vec();
            found.extend(tail);
            limit = found.len();
            best = Some(found);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{inv_seq, parse_moves};
    use crate::cuben::Geometry;

    #[test]
    fn every_side_change_lifts_through_later_turns_and_compensates_the_final_hold() {
        let shell = parse_moves("Rw U Fw2 L' Uw B").unwrap();
        let g = Geometry::new(5);
        for mask in 0..8 {
            let mut hold = 0;
            let mut bits = mask;
            let mut physical = Vec::new();
            for &m in &shell {
                let choices = &transitions()[hold][m as usize];
                let side = if m < 18 {
                    0
                } else {
                    let side = bits & 1;
                    bits >>= 1;
                    side
                };
                physical.push(choices[side].physical);
                hold = choices[side].hold;
            }
            let scramble = inv_seq(&physical);
            let mut at = Cube5::solved();
            at.apply_seq(&scramble);
            let found = lift_word(&at, &shell).unwrap();
            assert_eq!(found.len(), shell.len());
            let mut stickers = g.solved();
            for m in scramble.iter().chain(&found) {
                g.apply(&mut stickers, *m as usize);
            }
            assert!(g.is_solved(&stickers));
            assert!(lift_word(&at, &shell[..shell.len() - 1]).is_none());
        }
    }
}
