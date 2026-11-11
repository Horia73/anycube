//! Input checks: is this a cube, and can it be solved?
//!
//! A scramble always gives a solvable cube, so only its syntax is checked.
//! Facelets come from a painted picture or a camera and can be anything; a
//! set of stickers is a real cube when
//!
//! - every colour appears on N*N stickers,
//! - on a cube of odd size the middle of each face has its own colour,
//! - each corner is one of the eight corner pieces, each once, and their
//!   twists add up to a multiple of three,
//! - each edge piece is a piece that exists, each once (a wing cannot be
//!   turned over in its place, so its two colours say which of the two
//!   wings of an edge it is), and the flips of the middle edges add up to
//!   an even number,
//! - on a cube of odd size the corners and the middle edges are both an
//!   even or both an odd permutation,
//! - the centres that can reach each other's places hold four of each colour.
//!
//! Those are all the conditions there are: wings and centres of a kind can
//! be arranged in every way that passes them.

use crate::cuben::Geometry;
use std::collections::{HashMap, HashSet};

use crate::api::{HARD_MAX_SIZE, MAX_SCRAMBLE_MOVES};

/// What is wrong with a request, as a stable code and a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError { code, message: message.into() }
    }

    /// Whether the code says that the stickers are not those of a real cube
    /// (as opposed to a request that is malformed).
    pub fn impossible(&self) -> bool {
        matches!(
            self.code,
            "colour_count"
                | "fixed_centres"
                | "centre_colours"
                | "bad_corner"
                | "duplicate_corner"
                | "twisted_corner"
                | "bad_edge"
                | "duplicate_edge"
                | "flipped_edge"
                | "swapped_pieces"
        )
    }
}

pub enum Input<'a> {
    /// Face turns and wide turns: `U R2 F' Rw 3Uw2`.
    Moves(&'a str),
    /// 6*N*N letters, the faces in URFDLB order, each row by row.
    Facelets(&'a str),
}

/// A request that passed every check.
#[derive(Debug, Clone)]
pub struct Checked {
    pub size: usize,
    /// The stickers, 0 to 5 for URFDLB.
    pub facelets: Vec<u8>,
    /// The scramble as the solver names moves, when the cube came as moves.
    pub scramble: Option<String>,
}

const FACE_NAMES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

/// Outward normal, column direction and row direction of each face; the
/// same frame as `anycube::cuben` (a test holds the two together).
const FACES: [([i32; 3], [i32; 3], [i32; 3]); 6] = [
    ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
    ([1, 0, 0], [0, 0, -1], [0, -1, 0]),
    ([0, 0, 1], [1, 0, 0], [0, -1, 0]),
    ([0, -1, 0], [1, 0, 0], [0, 0, -1]),
    ([-1, 0, 0], [0, 0, 1], [0, -1, 0]),
    ([0, 0, -1], [-1, 0, 0], [0, -1, 0]),
];

fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Where a facelet is: the middle of its little cube (in half units, so
/// that it is integral), its face, and its place on the face measured from
/// the middle of the face.
pub(crate) struct Sticker {
    pub cubie: [i32; 3],
    pub face: usize,
    pub a: i32,
    pub b: i32,
}

pub(crate) fn sticker(n: usize, i: usize) -> Sticker {
    let nn = n as i32;
    let (face, r, c) = (i / (n * n), (i / n % n) as i32, (i % n) as i32);
    let (normal, right, down) = FACES[face];
    let (a, b) = (2 * c - (nn - 1), 2 * r - (nn - 1));
    let mut cubie = [0; 3];
    for k in 0..3 {
        cubie[k] = normal[k] * (nn - 1) + right[k] * a + down[k] * b;
    }
    Sticker { cubie, face, a, b }
}

/// Checks a request and returns the cube it describes.
pub fn check(size: usize, input: Input, max_size: usize) -> Result<Checked, ApiError> {
    let max = max_size.min(HARD_MAX_SIZE);
    if size < 2 || size > max {
        return Err(ApiError::new("bad_size", format!("the size must be between 2 and {}, got {}", max, size)));
    }
    let g = Geometry::new(size);
    match input {
        Input::Moves(text) => {
            let moves = parse_scramble(&g, text)?;
            let mut facelets = g.solved();
            g.apply_seq(&mut facelets, &moves);
            let names: Vec<String> = moves.iter().map(|&m| g.move_name(m)).collect();
            Ok(Checked { size, facelets, scramble: Some(names.join(" ")) })
        }
        Input::Facelets(text) => {
            let facelets = parse_facelets(size, text)?;
            validate(size, &facelets)?;
            Ok(Checked { size, facelets, scramble: None })
        }
    }
}

fn parse_scramble(g: &Geometry, text: &str) -> Result<Vec<usize>, ApiError> {
    let count = text.split_whitespace().count();
    if count > MAX_SCRAMBLE_MOVES {
        return Err(ApiError::new(
            "bad_moves",
            format!("the scramble has {} moves; the longest accepted is {}", count, MAX_SCRAMBLE_MOVES),
        ));
    }
    if let Some(tok) = text.split_whitespace().find(|t| t.len() > 8) {
        let shown: String = tok.chars().take(8).collect();
        return Err(ApiError::new("bad_moves", format!("'{}...' is not a move", shown)));
    }
    g.parse_moves(text).map_err(|e| {
        let widest = match g.n / 2 {
            1 => "face turns such as U, R2 and F'".to_string(),
            2 => "face turns such as U, R2 and F', and wide turns such as Rw".to_string(),
            k => format!("face turns such as U, R2 and F', and wide turns from Rw to {}Rw", k),
        };
        ApiError::new("bad_moves", format!("{}; this size takes {}", e, widest))
    })
}

fn parse_facelets(n: usize, text: &str) -> Result<Vec<u8>, ApiError> {
    let b = text.trim().as_bytes();
    if b.len() != 6 * n * n {
        return Err(ApiError::new(
            "bad_facelets",
            format!("a {0}x{0}x{0} has {1} facelets (6 faces of {0} by {0}), got {2}", n, 6 * n * n, b.len()),
        ));
    }
    b.iter()
        .enumerate()
        .map(|(i, c)| {
            b"URFDLB".iter().position(|x| x == c).map(|p| p as u8).ok_or_else(|| {
                ApiError::new(
                    "bad_facelets",
                    format!("facelet {} is not one of the letters U, R, F, D, L and B", i + 1),
                )
            })
        })
        .collect()
}

fn axis(colour: u8) -> u8 {
    colour % 3
}

fn names(colours: &[u8]) -> String {
    let v: Vec<String> = colours.iter().map(|&c| FACE_NAMES[c as usize].to_string()).collect();
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        k => format!("{} and {}", v[..k - 1].join(", "), v[k - 1]),
    }
}

/// The three (face, colour) pairs of a corner, the U or D face first and
/// the other two in the same sense round the corner for every corner.
fn in_order(mut t: [(usize, u8); 3]) -> [(usize, u8); 3] {
    let k = t.iter().position(|x| x.0 % 3 == 0).expect("every corner touches U or D");
    t.rotate_left(k);
    if dot(FACES[t[0].0].0, cross(FACES[t[1].0].0, FACES[t[2].0].0)) > 0 {
        t.swap(1, 2);
    }
    t
}

/// 0 for an even permutation, 1 for an odd one.
fn parity(p: &[usize]) -> usize {
    let mut seen = vec![false; p.len()];
    let mut odd = 0;
    for i in 0..p.len() {
        let (mut j, mut len) = (i, 0);
        while !seen[j] {
            seen[j] = true;
            j = p[j];
            len += 1;
        }
        if len > 0 {
            odd ^= (len + 1) & 1;
        }
    }
    odd
}

/// Whether these stickers are those of a cube that can be solved.
pub fn validate(n: usize, f: &[u8]) -> Result<(), ApiError> {
    assert_eq!(f.len(), 6 * n * n);
    let nn = n as i32;
    let rim = nn - 1;

    let mut count = [0usize; 6];
    for &c in f {
        count[c as usize] += 1;
    }
    if count.iter().any(|&k| k != n * n) {
        let off: Vec<String> = (0..6)
            .filter(|&c| count[c] != n * n)
            .map(|c| format!("{} of {}", count[c], FACE_NAMES[c]))
            .collect();
        return Err(ApiError::new(
            "colour_count",
            format!("every colour must be on {} stickers; there are {}", n * n, off.join(", ")),
        ));
    }

    // Sort the stickers by the kind of piece they are on.
    let mut corners: [Vec<(usize, u8)>; 8] = Default::default();
    let mut edges: HashMap<[i32; 3], Vec<(usize, u8)>> = HashMap::new();
    let mut centres: HashMap<(i32, i32), [usize; 6]> = HashMap::new();
    for (i, &colour) in f.iter().enumerate() {
        let s = sticker(n, i);
        match (s.a.abs() == rim, s.b.abs() == rim) {
            (true, true) => {
                let slot = (0..3).fold(0, |id, k| id | ((s.cubie[k] > 0) as usize) << k);
                corners[slot].push((s.face, colour));
            }
            (false, false) => {
                // A quarter turn of the face takes (a, b) to (-b, a): the four
                // places are one kind of centre, and they look alike on every face.
                let (a, b) = (s.a, s.b);
                let kind = [(a, b), (-b, a), (-a, -b), (b, -a)].into_iter().max().unwrap();
                if kind == (0, 0) {
                    if colour as usize != s.face {
                        return Err(ApiError::new(
                            "fixed_centres",
                            format!(
                                "the middle of the {} face is {}; on a cube of odd size the middles do not move, so each names its face",
                                FACE_NAMES[s.face], FACE_NAMES[colour as usize]
                            ),
                        ));
                    }
                } else {
                    centres.entry(kind).or_insert([0; 6])[colour as usize] += 1;
                }
            }
            _ => edges.entry(s.cubie).or_default().push((s.face, colour)),
        }
    }

    // Corners.
    let home_slot = |colours: [u8; 3]| -> usize {
        colours.iter().fold(0, |id, &c| {
            let normal = FACES[c as usize].0;
            let k = (0..3).find(|&k| normal[k] != 0).unwrap();
            id | ((normal[k] > 0) as usize) << k
        })
    };
    let mut corner_perm = [0usize; 8];
    let mut corner_seen = [false; 8];
    let mut twist = 0;
    for (slot, stickers) in corners.iter().enumerate() {
        let t = in_order([stickers[0], stickers[1], stickers[2]]);
        let colours = [t[0].1, t[1].1, t[2].1];
        let mut axes = [axis(colours[0]), axis(colours[1]), axis(colours[2])];
        axes.sort_unstable();
        if axes != [0, 1, 2] {
            return Err(ApiError::new(
                "bad_corner",
                format!("no corner piece has the colours {} together", names(&colours)),
            ));
        }
        // The piece these colours belong to, read in the same order at its home.
        let home = in_order([
            (colours[0] as usize, colours[0]),
            (colours[1] as usize, colours[1]),
            (colours[2] as usize, colours[2]),
        ]);
        let k = colours.iter().position(|&c| c == home[0].1).unwrap();
        if colours[(k + 1) % 3] != home[1].1 || colours[(k + 2) % 3] != home[2].1 {
            return Err(ApiError::new(
                "bad_corner",
                format!(
                    "the corner with {} has its colours in mirror order; two of its stickers are exchanged",
                    names(&colours)
                ),
            ));
        }
        let piece = home_slot(colours);
        if std::mem::replace(&mut corner_seen[piece], true) {
            return Err(ApiError::new(
                "duplicate_corner",
                format!("the corner with {} appears twice", names(&colours)),
            ));
        }
        corner_perm[slot] = piece;
        twist += k;
    }

    // Edges: the twelve middle ones of an odd cube, and the wings.
    let pairs: Vec<(usize, usize)> =
        (0..6).flat_map(|a| (a + 1..6).map(move |b| (a, b))).filter(|&(a, b)| a % 3 != b % 3).collect();
    let pair_index = |a: usize, b: usize| pairs.iter().position(|&p| p == (a, b)).unwrap();
    let mut edge_seen: HashSet<(usize, usize, i32)> = HashSet::new();
    let mut middle_perm = [0usize; 12];
    let mut flips = 0;
    for (cubie, stickers) in &edges {
        let (mut x, mut y) = (stickers[0], stickers[1]);
        if x.0 > y.0 {
            std::mem::swap(&mut x, &mut y);
        }
        if axis(x.1) == axis(y.1) {
            return Err(ApiError::new(
                "bad_edge",
                if x.1 == y.1 {
                    format!("an edge piece has {} on both of its stickers", FACE_NAMES[x.1 as usize])
                } else {
                    format!("no edge piece has the colours {} together", names(&[x.1, y.1]))
                },
            ));
        }
        // How far along its edge the piece sits, with a sign that turns
        // with the piece: the same number at its home, read off its colours.
        let along = dot(*cubie, cross(FACES[x.0].0, FACES[y.0].0));
        let piece = if x.1 < y.1 { (x.1 as usize, y.1 as usize, along) } else { (y.1 as usize, x.1 as usize, -along) };
        if !edge_seen.insert(piece) {
            return Err(ApiError::new(
                "duplicate_edge",
                format!(
                    "the {} with {} appears twice",
                    if along == 0 { "edge" } else { "edge piece" },
                    names(&[x.1, y.1])
                ),
            ));
        }
        if along == 0 {
            middle_perm[pair_index(x.0, y.0)] = pair_index(piece.0, piece.1);
            // The sticker that counts: on U or D if there is one, else on F or B.
            let lead = |p: u8, q: u8| if p % 3 == 0 || (q % 3 != 0 && p % 3 == 2) { p } else { q };
            let face = lead(x.0 as u8, y.0 as u8);
            let shown = if face as usize == x.0 { x.1 } else { y.1 };
            flips += (shown != lead(x.1, y.1)) as usize;
        }
    }

    // Centres.
    for (kind, counts) in &centres {
        if let Some(c) = (0..6).find(|&c| counts[c] != 4) {
            return Err(ApiError::new(
                "centre_colours",
                format!(
                    "the centres {} from the middle of their faces must be four of each colour; there are {} of {}",
                    match (kind.0.abs().min(kind.1.abs()), kind.0.abs().max(kind.1.abs())) {
                        (lo, hi) if lo == hi => format!("{} steps diagonally", (lo + 1) / 2),
                        (0, hi) => format!("{} steps straight", hi / 2),
                        (lo, hi) => format!("{} steps one way and {} the other", (hi + 1) / 2, (lo + 1) / 2),
                    },
                    counts[c],
                    FACE_NAMES[c]
                ),
            ));
        }
    }

    // What cannot be seen on a single piece.
    if twist % 3 != 0 {
        return Err(ApiError::new("twisted_corner", "one corner is twisted in its place"));
    }
    if flips % 2 != 0 {
        return Err(ApiError::new("flipped_edge", "one edge is flipped in its place"));
    }
    if n % 2 == 1 && parity(&corner_perm) != parity(&middle_perm) {
        return Err(ApiError::new("swapped_pieces", "two pieces are exchanged: two corners, or two edges"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(size: usize, input: Input) -> &'static str {
        match check(size, input, 12) {
            Ok(_) => "ok",
            Err(e) => e.code,
        }
    }

    /// A random scramble of `len` moves.
    fn scrambled(g: &Geometry, seed: u64, len: usize) -> Vec<u8> {
        let mut seed = seed;
        let mut f = g.solved();
        for _ in 0..len {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            g.apply(&mut f, (seed >> 33) as usize % g.move_count());
        }
        f
    }

    /// The facelets of the piece at a corner, or at an edge, by face and place.
    fn at(n: usize, face: usize, r: usize, c: usize) -> usize {
        face * n * n + r * n + c
    }

    #[test]
    fn the_frame_is_the_solvers() {
        // A U turn moves the top row of F to the top row of L: true of the
        // solver's model, and of this module's picture of where stickers are.
        let g = Geometry::new(3);
        let mut f: Vec<u8> = (0..54).collect();
        g.apply(&mut f, 0);
        assert_eq!(f[at(3, 4, 0, 0)], at(3, 2, 0, 0) as u8);
        let (front, left) = (sticker(3, at(3, 2, 0, 0)), sticker(3, at(3, 4, 0, 0)));
        assert_eq!(front.cubie, [-2, 2, 2]);
        assert_eq!(left.cubie, [-2, 2, -2]);
        assert_eq!((front.face, left.face), (2, 4));
    }

    #[test]
    fn sizes() {
        assert_eq!(code(1, Input::Moves("U")), "bad_size");
        assert_eq!(code(13, Input::Moves("U")), "bad_size");
        assert_eq!(code(0, Input::Facelets("")), "bad_size");
        assert_eq!(code(2, Input::Moves("U")), "ok");
        assert_eq!(code(12, Input::Moves("6Uw")), "ok");
        assert_eq!(check(HARD_MAX_SIZE + 1, Input::Moves("U"), 1000).unwrap_err().code, "bad_size");
        assert_eq!(code(HARD_MAX_SIZE, Input::Moves("U")), "bad_size", "12 is the largest this test allows");
    }

    #[test]
    fn move_syntax() {
        assert_eq!(code(3, Input::Moves("")), "ok");
        assert_eq!(code(3, Input::Moves("R U R' U' F2 D L' B")), "ok");
        assert_eq!(code(3, Input::Moves("Rw")), "bad_moves");
        assert_eq!(code(3, Input::Moves("M")), "bad_moves");
        assert_eq!(code(3, Input::Moves("x")), "bad_moves");
        assert_eq!(code(4, Input::Moves("Rw r 2Rw U2 F'")), "ok");
        assert_eq!(code(4, Input::Moves("3Rw")), "bad_moves");
        assert_eq!(code(7, Input::Moves("3Rw 3Bw' Uw2")), "ok");
        assert_eq!(code(7, Input::Moves("4Rw")), "bad_moves");
        assert_eq!(code(5, Input::Moves("R3")), "bad_moves");
        assert_eq!(code(5, Input::Moves("R''''''''''''")), "bad_moves");
        assert_eq!(code(5, Input::Moves("99999999999999999999R")), "bad_moves");
        assert_eq!(code(5, Input::Moves("ț")), "bad_moves");
        assert_eq!(code(5, Input::Moves("0R")), "bad_moves");
        let long = "U ".repeat(MAX_SCRAMBLE_MOVES + 1);
        assert_eq!(code(5, Input::Moves(&long)), "bad_moves");
        let c = check(5, Input::Moves("r  u2\tF'"), 12).unwrap();
        assert_eq!(c.scramble.as_deref(), Some("Rw Uw2 F'"));
    }

    #[test]
    fn facelet_syntax() {
        let g = Geometry::new(3);
        let solved = g.to_string(&g.solved());
        assert_eq!(code(3, Input::Facelets(&solved)), "ok");
        assert_eq!(code(3, Input::Facelets(&format!("  {}\n", solved))), "ok");
        assert_eq!(code(3, Input::Facelets(&solved[1..])), "bad_facelets");
        assert_eq!(code(4, Input::Facelets(&solved)), "bad_facelets");
        assert_eq!(code(3, Input::Facelets(&solved.replace('U', "u"))), "bad_facelets");
        assert_eq!(code(3, Input::Facelets(&solved.replace('U', "X"))), "bad_facelets");
    }

    #[test]
    fn every_scrambled_cube_passes() {
        for n in 2..=9 {
            let g = Geometry::new(n);
            for seed in 0..12 {
                let f = scrambled(&g, seed * 97 + n as u64, 40 + 20 * n);
                validate(n, &f).unwrap_or_else(|e| panic!("size {} seed {}: {}", n, seed, e.message));
            }
        }
    }

    #[test]
    fn an_even_cube_may_be_held_any_way() {
        for n in [2, 4, 6] {
            let g = Geometry::new(n);
            let f = scrambled(&g, 5, 60);
            validate(n, &g.turned(&f, &[0, 1, 1, 2])).unwrap();
        }
        // An odd one has its centres to say which way up it is.
        let g = Geometry::new(5);
        assert_eq!(validate(5, &g.turned(&scrambled(&g, 5, 60), &[0])).unwrap_err().code, "fixed_centres");
    }

    #[test]
    fn impossible_cubes() {
        for n in 2..=7 {
            let g = Geometry::new(n);
            let last = n - 1;
            for seed in 0..4 {
                let f = scrambled(&g, seed + 31 * n as u64, 30 * n);
                let broken = |change: &dyn Fn(&mut Vec<u8>)| {
                    let mut x = f.clone();
                    change(&mut x);
                    validate(n, &x).map_err(|e| e.code)
                };
                // The corner at U, R and F: its stickers moved round by one.
                let urf = [at(n, 0, last, last), at(n, 1, 0, 0), at(n, 2, 0, last)];
                let twisted = broken(&|x| {
                    let (a, b, c) = (x[urf[0]], x[urf[1]], x[urf[2]]);
                    (x[urf[0]], x[urf[1]], x[urf[2]]) = (c, a, b);
                });
                assert_eq!(twisted, Err("twisted_corner"), "size {}", n);
                // Two of its stickers exchanged: a mirror image of a corner.
                assert_eq!(broken(&|x| x.swap(urf[1], urf[2])), Err("bad_corner"), "size {}", n);
                // One sticker repainted.
                let repainted = broken(&|x| x[urf[0]] = (x[urf[0]] + 1) % 6);
                assert_eq!(repainted, Err("colour_count"), "size {}", n);
                // The corners at URF and UFL exchanged, sticker by sticker.
                let ufl = [at(n, 0, last, 0), at(n, 2, 0, 0), at(n, 4, 0, last)];
                let swapped = broken(&|x| {
                    for k in 0..3 {
                        x.swap(urf[k], ufl[k]);
                    }
                });
                // On an even cube that is a cube like any other.
                assert_eq!(swapped, if n % 2 == 1 { Err("swapped_pieces") } else { Ok(()) }, "size {}", n);
                if n % 2 == 1 {
                    let mid = n / 2;
                    let uf = [at(n, 0, last, mid), at(n, 2, 0, mid)];
                    assert_eq!(broken(&|x| x.swap(uf[0], uf[1])), Err("flipped_edge"), "size {}", n);
                    let ur = [at(n, 0, mid, last), at(n, 1, 0, mid)];
                    let two_edges = broken(&|x| {
                        x.swap(uf[0], ur[0]);
                        x.swap(uf[1], ur[1]);
                    });
                    assert_eq!(two_edges, Err("swapped_pieces"), "size {}", n);
                }
                if n >= 4 {
                    // A wing turned over where it is: it becomes the other wing of its edge.
                    let wing = [at(n, 0, last, 1), at(n, 2, 0, 1)];
                    assert_eq!(broken(&|x| x.swap(wing[0], wing[1])), Err("duplicate_edge"), "size {}", n);
                    // Two centres of different kinds exchanged, when their colours differ.
                    if n >= 5 {
                        let (p, q) = (at(n, 0, 1, 1), at(n, 2, 1, 2));
                        if f[p] != f[q] {
                            assert_eq!(broken(&|x| x.swap(p, q)), Err("centre_colours"), "size {}", n);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn two_wings_exchanged_is_a_cube() {
        // The parity case of the 4x4x4: it looks wrong and is not.
        let g = Geometry::new(4);
        let mut f = scrambled(&g, 3, 80);
        // Exchange the wings at UF with those at UR, place for place.
        let (uf, ur) = ([at(4, 0, 3, 1), at(4, 2, 0, 1)], [at(4, 0, 2, 3), at(4, 1, 0, 1)]);
        f.swap(uf[0], ur[0]);
        f.swap(uf[1], ur[1]);
        assert_eq!(validate(4, &f).map_err(|e| e.code), Ok(()));
    }

    #[test]
    fn messages_name_what_is_wrong() {
        let g = Geometry::new(3);
        let mut f = g.solved();
        f[0] = 1;
        let e = validate(3, &f).unwrap_err();
        assert!(e.impossible());
        assert!(e.message.contains("8 of U") && e.message.contains("10 of R"), "{}", e.message);
        assert!(!ApiError::new("bad_moves", "").impossible());
    }
}
