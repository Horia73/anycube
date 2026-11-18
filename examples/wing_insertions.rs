//! Complete 4x4x4 or 5x5x5 partial solutions that leave one wing three-cycle.
//! Each input line is scramble<TAB>partial solution. Successful output lines
//! can be passed straight to `anycube verify --size N`.
use anycube::cuben::Geometry;
use std::collections::HashMap;
use std::io::{self, BufRead};

fn inverse_move(m: usize) -> usize {
    usize::from(anycube::cube::inv_move(m as u8))
}

fn inverse(word: &[usize]) -> Vec<usize> {
    word.iter().rev().map(|&m| inverse_move(m)).collect()
}

fn simplify(mut word: Vec<usize>) -> Vec<usize> {
    loop {
        let mut next = Vec::with_capacity(word.len());
        for block in word.chunk_by(|a, b| a / 3 % 3 == b / 3 % 3) {
            let mut powers = [0; 12];
            for &m in block {
                powers[m / 3] += m % 3 + 1;
            }
            for (face, turns) in powers.into_iter().enumerate() {
                if turns % 4 != 0 {
                    next.push(face * 3 + turns % 4 - 1);
                }
            }
        }
        if next.len() == word.len() {
            return next;
        }
        word = next;
    }
}

fn compose(a: &[u8], b: &[u8]) -> Vec<u8> {
    b.iter().map(|&i| a[i as usize]).collect()
}

struct Algorithm {
    word: Vec<usize>,
    perm: Vec<u8>,
    undo: Vec<u8>,
}

impl Algorithm {
    fn new(g: &Geometry, word: Vec<usize>) -> Self {
        let mut perm: Vec<u8> = (0..6 * g.n * g.n).map(|i| i as u8).collect();
        g.apply_seq(&mut perm, &word);
        let mut undo = vec![0; perm.len()];
        for (i, &from) in perm.iter().enumerate() {
            undo[from as usize] = i as u8;
        }
        Self { word, perm, undo }
    }
}

struct Correction {
    word: Vec<usize>,
    // Destination and source for the six stickers on the three wings.
    changes: Vec<(usize, usize)>,
}

fn wing(n: usize, i: usize) -> bool {
    let (r, c) = (i / n % n, i % n);
    let edge = |x| x == 0 || x == n - 1;
    edge(r) != edge(c) && (n.is_multiple_of(2) || (r != n / 2 && c != n / 2))
}

fn corrections(g: &Geometry) -> Vec<Correction> {
    let single: Vec<_> = (0..36)
        .map(|m| vec![m])
        .chain((0..18).map(|m| vec![m + 18, inverse_move(m)]))
        .map(|word| Algorithm::new(g, word))
        .collect();
    // B = x y x'. Equivalent setups share the cheapest word seen.
    let mut conjugates = HashMap::<Vec<u8>, Vec<usize>>::new();
    for x in &single {
        for y in &single {
            let perm = compose(&compose(&x.perm, &y.perm), &x.undo);
            let word = simplify([x.word.as_slice(), &y.word, &inverse(&x.word)].concat());
            let entry = conjugates.entry(perm).or_insert_with(|| word.clone());
            if (word.len(), &word) < (entry.len(), entry) {
                *entry = word;
            }
        }
    }
    let mut cycles = HashMap::<Vec<(usize, usize)>, Vec<usize>>::new();
    for word in conjugates.into_values() {
        let b = Algorithm::new(g, word);
        for a in &single {
            let perm = compose(&compose(&compose(&a.perm, &b.perm), &a.undo), &b.undo);
            let changes: Vec<_> = perm
                .iter()
                .enumerate()
                .filter_map(|(to, &from)| (to != from as usize).then_some((to, from as usize)))
                .collect();
            if changes.len() != 6
                || !changes
                    .iter()
                    .all(|&(to, from)| wing(g.n, to) && perm[perm[from] as usize] as usize == to)
            {
                continue;
            }
            let word = simplify(
                [
                    a.word.as_slice(),
                    &b.word,
                    &inverse(&a.word),
                    &inverse(&b.word),
                ]
                .concat(),
            );
            let entry = cycles.entry(changes).or_insert_with(|| word.clone());
            if (word.len(), &word) < (entry.len(), entry) {
                *entry = word;
            }
        }
    }
    let mut out: Vec<_> = cycles
        .into_iter()
        .map(|(changes, word)| Correction { changes, word })
        .collect();
    out.sort_by(|a, b| a.word.cmp(&b.word));
    out
}

fn repair(
    g: &Geometry,
    library: &[Correction],
    scramble: &[usize],
    partial: &[usize],
) -> Option<Vec<usize>> {
    let mut actual = g.solved();
    g.apply_seq(&mut actual, scramble);
    g.apply_seq(&mut actual, partial);
    if g.is_solved(&actual) {
        return Some(simplify(partial.to_vec()));
    }
    let area = g.n * g.n;
    let mut goal: Vec<_> = (0..actual.len()).map(|i| actual[i / area * area]).collect();
    let wrong = actual.iter().zip(&goal).filter(|(a, b)| a != b).count();
    if wrong > 6 || (0..actual.len()).any(|i| actual[i] != goal[i] && !wing(g.n, i)) {
        return None;
    }
    let mut best: Option<Vec<usize>> = None;
    // Undo the suffix on both states. At a cut, a correction must take
    // actual to goal while touching only its six known stickers.
    for cut in (0..=partial.len()).rev() {
        for alg in library {
            if alg
                .changes
                .iter()
                .all(|&(to, from)| actual[from] == goal[to])
                && alg
                    .changes
                    .iter()
                    .filter(|&&(to, _)| actual[to] != goal[to])
                    .count()
                    == wrong
            {
                let word = simplify([&partial[..cut], &alg.word, &partial[cut..]].concat());
                if best
                    .as_ref()
                    .is_none_or(|w| (word.len(), &word) < (w.len(), w))
                {
                    best = Some(word);
                }
            }
        }
        if cut > 0 {
            let undo = inverse_move(partial[cut - 1]);
            g.apply(&mut actual, undo);
            g.apply(&mut goal, undo);
        }
    }
    if let Some(word) = &best {
        g.apply_seq(&mut actual, word);
        assert!(g.is_solved(&actual), "insertion failed sticker replay");
    }
    best
}

fn main() -> Result<(), String> {
    let n = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .filter(|&n| n == 4 || n == 5)
        .ok_or("usage: wing_insertions 4|5 < partial-solutions.tsv")?;
    let g = Geometry::new(n);
    let library = corrections(&g);
    let mut misses = 0;
    for (i, line) in io::stdin().lock().lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() && !line.contains('\t') {
            continue;
        }
        let (s, w) = line.split_once('\t').ok_or_else(|| {
            format!(
                "line {} needs a tab between scramble and partial solution",
                i + 1
            )
        })?;
        let scramble = g.parse_moves(s)?;
        let partial = g.parse_moves(w)?;
        match repair(&g, &library, &scramble, &partial) {
            Some(word) => println!(
                "{}\t{}",
                s,
                word.into_iter()
                    .map(|m| g.move_name(m))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            None => {
                eprintln!("line {} has no correction in this library", i + 1);
                misses += 1;
            }
        }
    }
    if misses > 0 {
        Err(format!("{misses} partial solutions had no correction"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wing_cycle_inserted_inside_the_word_solves_both_sizes() {
        for n in [4, 5] {
            let g = Geometry::new(n);
            let library = corrections(&g);
            assert_eq!(library.len(), 1920);
            assert_eq!(repair(&g, &library, &[], &[]), Some(Vec::new()));
            let cycle = &library[0].word;
            let prefix = [3, inverse_move(cycle[0])];
            let suffix = [inverse_move(*cycle.last().unwrap()), 6, 9, 0];
            let complete = [prefix.as_slice(), cycle, &suffix].concat();
            let partial = [prefix.as_slice(), &suffix].concat();
            let solved = repair(&g, &library, &inverse(&complete), &partial).unwrap();
            assert!(solved.len() <= simplify(complete).len());
            assert!(repair(&g, &library, &[0], &[]).is_none());
        }
    }
}
