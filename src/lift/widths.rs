//! Each width is one Boolean shared by every piece. For each piece, a path
//! through the known turns must connect its initial slot to a goal slot.
//! Propagate those constraints from both ends before branching on a width.

use super::{goal, spellings, Choice, Domains, Orbits};

/// Outcome for one supplied projected word. Only a completed search can
/// rule the word out.
#[derive(Debug, PartialEq, Eq)]
pub enum Widths {
    Found(Vec<u8>),
    Impossible,
    BudgetExhausted,
}

const NARROW: u8 = 1;
const WIDE: u8 = 2;
const BOTH: u8 = NARROW | WIDE;
const ALL_PIECES: u32 = (1 << 24) - 1;

struct Problem<'a> {
    turns: Vec<&'a [Choice]>,
    initial: Domains,
    target: Domains,
    orbits: usize,
}

// Each bit is a distinct starting position, including for centres sharing a
// colour. Move the set of possible occupants of every slot one step.
fn advance(sets: &Domains, perms: [&Orbits<u8>; 2], width: u8, orbits: usize) -> Domains {
    let mut next = [[0; 24]; 3];
    for o in 0..orbits {
        for (i, slot) in next[o].iter_mut().enumerate() {
            let narrow = sets[o][perms[0][o][i] as usize];
            let wide = sets[o][perms[1][o][i] as usize];
            *slot = match width {
                NARROW => narrow,
                WIDE => wide,
                _ => narrow | wide,
            };
        }
    }
    next
}

// Every slot needs an occupant and every original piece needs a path.
// The number of remaining possibilities breaks ties when both widths fit.
fn support(before: &Domains, after: &Domains, choice: &Choice, orbits: usize) -> Option<u32> {
    let mut count = 0;
    for o in 0..orbits {
        let mut seen = 0;
        for (&src, &allowed) in choice.src[o].iter().zip(&after[o]) {
            let occupants = before[o][src as usize] & allowed;
            if occupants == 0 {
                return None;
            }
            seen |= occupants;
            count += occupants.count_ones();
        }
        if seen != ALL_PIECES {
            return None;
        }
    }
    Some(count)
}

enum Decision {
    Impossible,
    Complete,
    Branch(usize, u8),
}

impl Problem<'_> {
    fn propagate(&self, widths: &mut [u8], forward: &mut [Domains]) -> Decision {
        forward[0] = self.initial;
        loop {
            for (i, pair) in self.turns.iter().enumerate() {
                forward[i + 1] = advance(&forward[i], [&pair[0].src, &pair[1].src], widths[i], self.orbits);
            }
            let mut changed = false;
            let mut branch = None;
            let mut back = self.target;
            for i in (0..widths.len()).rev() {
                let pair = self.turns[i];
                let old = widths[i];
                let viable = |width, choice| {
                    if old & width == 0 {
                        None
                    } else {
                        support(&forward[i], &back, &pair[choice], self.orbits)
                    }
                };
                match (viable(NARROW, 0), viable(WIDE, 1)) {
                    (None, None) => return Decision::Impossible,
                    (Some(_), None) => widths[i] = NARROW,
                    (None, Some(_)) => widths[i] = WIDE,
                    (Some(n), Some(w)) => {
                        let rank = (n.abs_diff(w), std::cmp::Reverse(n.min(w)));
                        if branch.is_none_or(|(best, _, _)| rank > best) {
                            branch = Some((rank, i, if n >= w { NARROW } else { WIDE }));
                        }
                    }
                }
                changed |= widths[i] != old;
                back = advance(&back, [&pair[0].dest, &pair[1].dest], widths[i], self.orbits);
            }
            if !changed {
                return branch.map_or(Decision::Complete, |(_, i, first)| Decision::Branch(i, first));
            }
        }
    }
}

pub(super) fn recover(pieces: &Orbits<u8>, five: bool, word: &[u8], mut budget: usize) -> Widths {
    let goal = goal(five);
    if word.is_empty() {
        return if *pieces == goal {
            Widths::Found(Vec::new())
        } else {
            Widths::Impossible
        };
    }
    let mut target = [[0; 24]; 3];
    for o in 0..3 {
        for (i, &colour) in goal[o].iter().enumerate() {
            for (j, &piece) in pieces[o].iter().enumerate() {
                if piece == colour {
                    target[o][i] |= 1 << j;
                }
            }
        }
    }
    let problem = Problem {
        turns: word.iter().map(|&m| &spellings().face[m as usize][..2]).collect(),
        initial: [std::array::from_fn(|i| 1 << i); 3],
        target,
        orbits: if five { 3 } else { 2 },
    };
    let mut forward = vec![problem.initial; word.len() + 1];
    let mut pending = vec![vec![BOTH; word.len()]];
    while let Some(mut flags) = pending.pop() {
        if budget == 0 {
            return Widths::BudgetExhausted;
        }
        budget -= 1;
        match problem.propagate(&mut flags, &mut forward) {
            Decision::Impossible => {}
            Decision::Complete => {
                let moves = word
                    .iter()
                    .zip(flags)
                    .map(|(&m, flag)| if flag == WIDE { m + 18 } else { m })
                    .collect();
                return Widths::Found(moves);
            }
            Decision::Branch(i, preferred) => {
                flags[i] = BOTH ^ preferred;
                pending.push(flags.clone());
                flags[i] = preferred;
                pending.push(flags);
            }
        }
    }
    Widths::Impossible
}
