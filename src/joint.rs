//! Phases 1 and 2 as one search: from any state, stage all centres, bring
//! the R/L centres into a finishable pattern, make the wing permutation even
//! and the dedges pairable, using every move.
//!
//! The exact centre table makes the centre part of the search perfect: only
//! sequences that really stage the centres in the given number of moves are
//! followed. The other conditions are tested at the end.

use crate::ctab::NEAR;
use crate::cube::{skip_move, tables as cube_tables, Cube};
use crate::solver::{P2Cand, Params, Solver, Variant};
use crate::tables::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering::Relaxed};

#[derive(Clone, Copy)]
struct Node {
    /// Positions of the R/L-, U/D- and R-coloured centres.
    rl: u32,
    ud: u32,
    r: u32,
    /// Parity of the wing permutation.
    parity: u8,
    /// Exact number of moves to stage the centres.
    d: u8,
    /// Moves that bring the centres one step closer, and moves that keep the
    /// distance, when known.
    closer: u32,
    same: u32,
    /// Positions of the centres the two partner variants call R-coloured.
    pr: [u32; 2],
}

const UNKNOWN: u32 = u32::MAX;

struct Shared<'a> {
    count: AtomicUsize,
    limit: usize,
    budget: AtomicI64,
    stop: AtomicBool,
    /// The node budget and the deadline only apply once this is true.
    have_endpoint: &'a AtomicBool,
    deadline: std::time::Instant,
    /// Statistics for the debug output.
    staged: AtomicUsize,
    tried: AtomicUsize,
    by_rem: [AtomicUsize; 9],
}

/// A variant in another frame whose centres are staged by the same sequences
/// (relabelled), so one search serves all three frames.
struct Partner {
    variant: usize,
    /// Where this variant's R-coloured centres are in the searched variant.
    r: u32,
    /// Its R and L faces are this byte of the searched variant's masks ...
    shift: u32,
    /// ... and these patterns on them are the ones phase 3 can finish.
    ok: [bool; 256],
    /// Move names in this variant's frame.
    relabel: [u8; 36],
}

/// What the last move of a sequence does to the wings, per frame.
struct LastMove {
    /// Per move and frame: the positions whose wing the move sends to the low half.
    to_low: [[u32; 3]; 27],
    /// Per move: the frames in which it is not a phase-3 move (bit mask).
    live: [u8; 27],
    frames: usize,
}

struct Ctx<'a> {
    solver: &'a Solver,
    shared: &'a Shared<'a>,
    variants: &'a [Variant],
    partners: &'a [Partner],
    last_move: &'a LastMove,
    variant: usize,
    path: [u8; 32],
    out: Vec<P2Cand>,
    nodes: u64,
    /// Frames in which the sequence that just ended can be an endpoint (bit mask).
    frames_ok: u8,
    by_rem: [usize; 9],
}

const BATCH: u64 = 1 << 12;

impl Solver {
    fn joint_root(&self, cube: &Cube) -> Node {
        let m = axis_masks(cube);
        let mut r = 0u32;
        for i in 0..24 {
            if cube.ct[i] == 1 {
                r |= 1 << i;
            }
        }
        let d = self.ctab.exact(&self.tabs.bp, &self.tabs.ranks, m[2], m[0]);
        Node { rl: m[2], ud: m[0], r, parity: cube.wing_parity(), d, closer: UNKNOWN, same: UNKNOWN, pr: [0; 2] }
    }

    /// The partners of searched variant `i` in the other two frames.
    fn joint_partners(&self, variants: &[Variant], i: usize) -> Vec<Partner> {
        let ct = cube_tables();
        const GOALS: [u32; 3] = [0x0000FF, 0x00FF00, 0xFF0000];
        let mut out = Vec::new();
        for f in 1..3 {
            let frame = variants[f * 6].frame;
            let rot = ct.rots[frame].seq.clone();
            // The searched variant seen from the other frame: same colours, rotated positions.
            let mut seen = variants[i].cube;
            seen.apply_seq(&rot);
            let seen_masks = axis_masks(&seen);
            // Where the rotation takes each face pair.
            let mut turned = Cube::solved();
            turned.apply_seq(&rot);
            let turned_masks = axis_masks(&turned);
            let to_axis = |a: usize| GOALS.iter().position(|&g| g == turned_masks[a]).unwrap();
            let j = (f * 6..f * 6 + 6)
                .find(|&j| {
                    let m = axis_masks(&variants[j].cube);
                    (0..3).all(|a| m[to_axis(a)] == seen_masks[a])
                })
                .expect("every variant has a partner in each frame");
            // The colour of the searched variant that the partner calls R.
            let p = (0..24).find(|&p| variants[j].cube.ct[p] == 1).unwrap();
            let colour = seen.ct[p];
            let mut r = 0u32;
            for q in 0..24 {
                if variants[i].cube.ct[q] == colour {
                    r |= 1 << q;
                }
            }
            // The face pair of the searched variant that the rotation takes to R/L.
            let bp = &self.tabs.bp;
            let turn = |mask: u32| rot.iter().fold(mask, |x, &m| bp.apply(m, x));
            let shift = [0u32, 8, 16].into_iter().find(|&s| turn(0xFF << s) == 0xFF_0000).unwrap();
            let mut ok = [false; 256];
            for (b, o) in ok.iter_mut().enumerate() {
                *o = self.rl_ok[(turn((b as u32) << shift) >> 16) as usize];
            }
            out.push(Partner { variant: j, r, shift, ok, relabel: ct.relabel[ct.rot_inv[frame]] });
        }
        out
    }

    /// Searches below `node` with `rem` moves left. The upper levels of the tree fan out
    /// over the thread pool; the deep, cheap end runs sequentially.
    #[allow(clippy::too_many_arguments)]
    fn joint_par(
        &self,
        shared: &Shared,
        variants: &[Variant],
        partners: &[Partner],
        last_move: &LastMove,
        variant: usize,
        node: &Node,
        rem: usize,
        prefix: &[u8],
    ) -> Vec<P2Cand> {
        let last = prefix.last().copied().unwrap_or(255);
        if rem <= 7 {
            let mut ctx = Ctx {
                solver: self,
                shared,
                variants,
                partners,
                last_move,
                variant,
                path: [0; 32],
                out: Vec::new(),
                nodes: 0,
                frames_ok: 7,
                by_rem: [0; 9],
            };
            ctx.path[..prefix.len()].copy_from_slice(prefix);
            ctx.dfs(node, rem, last, prefix.len());
            for (a, &n) in shared.by_rem.iter().zip(ctx.by_rem.iter()) {
                a.fetch_add(n, Relaxed);
            }
            return ctx.out;
        }
        if shared.stop.load(Relaxed) {
            return Vec::new();
        }
        let children: Vec<(u8, Node)> = P1_MOVES
            .iter()
            .filter(|&&m| last == 255 || !skip_move(last, m))
            .filter_map(|&m| self.joint_step(node, m, rem - 1).map(|n| (m, n)))
            .collect();
        children
            .par_iter()
            .flat_map_iter(|(m, n)| {
                let mut path = prefix.to_vec();
                path.push(*m);
                self.joint_par(shared, variants, partners, last_move, variant, n, rem - 1, &path)
            })
            .collect()
    }

    /// The node after move `m`, if the centres can still be staged in `rem` more moves.
    #[inline(always)]
    fn joint_step(&self, n: &Node, m: u8, rem: usize) -> Option<Node> {
        // A sequence that leaves the wing permutation odd is no endpoint in any frame.
        let parity = n.parity ^ p2_flips_parity(m) as u8;
        if rem == 0 && parity != 0 {
            return None;
        }
        let bp = &self.tabs.bp;
        let (rl, ud) = (bp.apply(m, n.rl), bp.apply(m, n.ud));
        let (d, closer, same) = if rem <= NEAR {
            // Close to the goal the small in-cache table answers exactly.
            match self.near.get(rl, ud) {
                Some((d, closer, same)) if d as usize <= rem => (d, closer, same),
                _ => return None,
            }
        } else {
            // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
            let v = self.ctab.get(self.ctab.index(&self.tabs.ranks, rl, ud));
            let d = n.d + (v + 3 - (n.d + 2) % 3) % 3 - 1;
            if d as usize > rem {
                return None;
            }
            (d, UNKNOWN, UNKNOWN)
        };
        Some(Node {
            rl,
            ud,
            r: bp.apply(m, n.r),
            parity,
            d,
            closer,
            same,
            pr: [bp.apply(m, n.pr[0]), bp.apply(m, n.pr[1])],
        })
    }

    /// Endpoints of the joint phase, by increasing length, until `p.n2` are found
    /// or the node budget is spent.
    pub(crate) fn joint_endpoints(
        &self,
        variants: &[Variant],
        p: &Params,
        deadline: std::time::Instant,
    ) -> Result<Vec<P2Cand>, String> {
        // Variants come in groups of six per frame. Only the first frame is searched;
        // every staged sequence is then also tried in its partner of the other two frames.
        let share = std::env::var_os("CUBENXNXN_NO_SHARE").is_none();
        let searched = if share { 6 } else { variants.len() };
        let partners: Vec<Vec<Partner>> = (0..searched)
            .map(|i| if share { self.joint_partners(variants, i) } else { Vec::new() })
            .collect();
        let roots: Vec<Node> = (0..searched)
            .map(|i| {
                let mut n = self.joint_root(&variants[i].cube);
                for (k, part) in partners[i].iter().enumerate() {
                    n.pr[k] = part.r;
                }
                n
            })
            .collect();
        let lower = roots.iter().map(|n| n.d as usize).min().unwrap();
        // For the test on the last move: which wings each move sends to the low half, per frame.
        let ct = cube_tables();
        let mut last_move = LastMove { to_low: [[0; 3]; 27], live: [0; 27], frames: if share { 3 } else { 1 } };
        for f in 0..last_move.frames {
            // Positions that end in the low half when the cube is turned into frame `f`.
            let mut at = [0u8; 24];
            for (i, a) in at.iter_mut().enumerate() {
                *a = i as u8;
            }
            let frame = variants[f * 6].frame;
            for &m in &ct.rots[frame].seq {
                let old = at;
                for i in 0..24 {
                    at[i] = old[ct.ep_src[m as usize][i] as usize];
                }
            }
            let low = at[..12].iter().fold(0u32, |a, &p| a | 1 << p);
            let relabel = &ct.relabel[ct.rot_inv[frame]];
            for &m in &P1_MOVES {
                let src = &ct.ep_src[m as usize];
                for q in 0..24 {
                    if (low >> q) & 1 == 1 {
                        last_move.to_low[m as usize][f] |= 1 << src[q];
                    }
                }
                if !is_p3_move(relabel[m as usize]) {
                    last_move.live[m as usize] |= 1 << f;
                }
            }
        }
        // Development aid: stop after a given depth.
        let max_depth: Option<usize> =
            std::env::var("CUBENXNXN_JOINT_MAXDEPTH").ok().and_then(|s| s.parse().ok());
        let have_endpoint = AtomicBool::new(false);
        let shared = Shared {
            count: AtomicUsize::new(0),
            limit: p.n2,
            budget: AtomicI64::new(p.joint_nodes),
            stop: AtomicBool::new(false),
            have_endpoint: &have_endpoint,
            deadline,
            staged: AtomicUsize::new(0),
            tried: AtomicUsize::new(0),
            by_rem: Default::default(),
        };
        let mut found: Vec<P2Cand> = Vec::new();
        for depth in lower..=24 {
            let parts: Vec<Vec<P2Cand>> = roots
                .par_iter()
                .enumerate()
                .filter(|(_, root)| root.d as usize <= depth)
                .map(|(vi, root)| self.joint_par(&shared, variants, &partners[vi], &last_move, vi, root, depth, &[]))
                .collect();
            found.extend(parts.into_iter().flatten());
            if !found.is_empty() {
                have_endpoint.store(true, Relaxed);
            }
            let late = shared.budget.load(Relaxed) < 0 || std::time::Instant::now() > deadline;
            if found.len() >= p.n2 || (!found.is_empty() && late) {
                break;
            }
            if max_depth.is_some_and(|m| depth >= m) {
                break;
            }
            shared.stop.store(false, Relaxed);
        }
        if std::env::var_os("CUBENXNXN_DEBUG").is_some() {
            eprintln!(
                "debug: joint phase: nodes by moves left {:?}, {} staged sequences, {} edge tests, {} endpoints",
                shared.by_rem.iter().map(|a| a.load(Relaxed)).collect::<Vec<_>>(),
                shared.staged.load(Relaxed),
                shared.tried.load(Relaxed),
                found.len()
            );
        }
        Ok(found)
    }
}

impl Ctx<'_> {
    fn dfs(&mut self, n: &Node, rem: usize, last: u8, depth: usize) {
        if rem == 0 {
            self.leaf(n, depth);
            return;
        }
        self.nodes += 1;
        self.by_rem[rem.min(8)] += 1;
        if self.nodes % BATCH == 0 {
            let left = self.shared.budget.fetch_sub(BATCH as i64, Relaxed);
            let late = left < 0 || std::time::Instant::now() > self.shared.deadline;
            if late && self.shared.have_endpoint.load(Relaxed) {
                self.shared.stop.store(true, Relaxed);
            }
            if self.shared.stop.load(Relaxed) {
                return;
            }
        }
        // With no move to spare, only the moves that bring the centres closer can work;
        // with one to spare, also those that keep the distance.
        let allowed = match rem - n.d as usize {
            0 => n.closer,
            1 => n.closer | n.same,
            _ => UNKNOWN,
        };
        // Before the last move: where the two wings of each dedge are. Almost every
        // last move leaves some dedge with both wings in the same half, in every
        // frame, and can be skipped without looking at the centres.
        let mut pairs = [(0u8, 0u8); 12];
        if rem == 1 {
            let ct = cube_tables();
            let mut ep = self.variants[self.variant].cube.ep;
            for &m in &self.path[..depth] {
                let src = &ct.ep_src[m as usize];
                let old = ep;
                for i in 0..24 {
                    ep[i] = old[src[i] as usize];
                }
            }
            for (p, &piece) in ep.iter().enumerate() {
                let c = (piece % 12) as usize;
                if piece < 12 {
                    pairs[c].0 = p as u8;
                } else {
                    pairs[c].1 = p as u8;
                }
            }
        }
        for &m in &P1_MOVES {
            if (allowed >> m) & 1 == 0 || (last != 255 && skip_move(last, m)) {
                continue;
            }
            if rem == 1 {
                // An endpoint that ends with a phase-3 move is the previous endpoint plus
                // a move phase 3 could make itself.
                let lm = self.last_move;
                let mut ok = lm.live[m as usize];
                for f in 0..lm.frames {
                    if (ok >> f) & 1 == 1 {
                        let low = lm.to_low[m as usize][f];
                        if !pairs.iter().all(|&(p, q)| ((low >> p) ^ (low >> q)) & 1 == 1) {
                            ok &= !(1 << f);
                        }
                    }
                }
                if ok == 0 {
                    continue;
                }
                self.frames_ok = ok;
            }
            if let Some(next) = self.solver.joint_step(n, m, rem - 1) {
                self.path[depth] = m;
                self.dfs(&next, rem - 1, m, depth + 1);
                if self.shared.stop.load(Relaxed) {
                    return;
                }
            }
        }
    }

    /// The centres are staged; test everything else phase 3 needs, in all three frames.
    fn leaf(&mut self, n: &Node, depth: usize) {
        self.shared.staged.fetch_add(1, Relaxed);
        // The wing parity is the same in every frame.
        if n.parity != 0 {
            return;
        }
        let solver = self.solver;
        let last = if depth > 0 { Some(self.path[depth - 1]) } else { None };
        if self.frames_ok & 1 == 1 && solver.rl_ok[(n.r >> 16) as usize] && !last.is_some_and(is_p3_move) {
            self.endpoint(self.variant, depth, None);
        }
        let partners = self.partners;
        for (k, part) in partners.iter().enumerate() {
            if (self.frames_ok >> (k + 1)) & 1 == 1
                && part.ok[((n.pr[k] >> part.shift) & 255) as usize]
                && !last.is_some_and(|m| is_p3_move(part.relabel[m as usize]))
            {
                self.endpoint(part.variant, depth, Some(&part.relabel));
            }
        }
    }

    /// Tests the current sequence, in the frame of `variant`, for pairable dedges
    /// and records it as an endpoint.
    fn endpoint(&mut self, variant: usize, depth: usize, relabel: Option<&[u8; 36]>) {
        self.shared.tried.fetch_add(1, Relaxed);
        let tabs = &self.solver.tabs;
        let mut moves = [0u8; 32];
        for (i, &m) in self.path[..depth].iter().enumerate() {
            moves[i] = relabel.map_or(m, |r| r[m as usize]);
        }
        let moves = &moves[..depth];
        // Almost every candidate fails on the wings, so look at those alone first.
        let ct = cube_tables();
        let mut ep = self.variants[variant].cube.ep;
        for &m in moves {
            let src = &ct.ep_src[m as usize];
            let old = ep;
            for i in 0..24 {
                ep[i] = old[src[i] as usize];
            }
        }
        if !wings_pairable(&ep) {
            return;
        }
        let mut cube = self.variants[variant].cube;
        cube.apply_seq(moves);
        debug_assert_eq!(tabs.p2[tabs.p2_index(p2_mask(&cube), cube.wing_parity())], 0);
        let dc = tabs.ct3[tabs.ct3_index(ct3_mask(&cube), ct3_parity(&cube))];
        if dc == UNREACHED {
            return;
        }
        let ds = tabs.sigma[sigma_rank(&sigma_of(&cube))];
        let len = moves.len() as u8;
        self.out.push(P2Cand { variant, len1: len, moves: moves.to_vec(), cube, len12: len, value: len + dc.max(ds) });
        self.shared.have_endpoint.store(true, Relaxed);
        if self.shared.count.fetch_add(1, Relaxed) + 1 >= self.shared.limit {
            self.shared.stop.store(true, Relaxed);
        }
    }
}
