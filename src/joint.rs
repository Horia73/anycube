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
    /// Moves that bring the centres one step closer, when known.
    closer: u32,
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

struct Ctx<'a> {
    solver: &'a Solver,
    shared: &'a Shared<'a>,
    variants: &'a [Variant],
    partners: &'a [Partner],
    variant: usize,
    path: [u8; 32],
    out: Vec<P2Cand>,
    nodes: u64,
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
        Node { rl: m[2], ud: m[0], r, parity: cube.wing_parity(), d, closer: UNKNOWN, pr: [0; 2] }
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
        variant: usize,
        node: &Node,
        rem: usize,
        prefix: &[u8],
    ) -> Vec<P2Cand> {
        let last = prefix.last().copied().unwrap_or(255);
        if rem <= 7 {
            let mut ctx = Ctx { solver: self, shared, variants, partners, variant, path: [0; 32], out: Vec::new(), nodes: 0 };
            ctx.path[..prefix.len()].copy_from_slice(prefix);
            ctx.dfs(node, rem, last, prefix.len());
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
                self.joint_par(shared, variants, partners, variant, n, rem - 1, &path)
            })
            .collect()
    }

    /// The node after move `m`, if the centres can still be staged in `rem` more moves.
    #[inline(always)]
    fn joint_step(&self, n: &Node, m: u8, rem: usize) -> Option<Node> {
        let bp = &self.tabs.bp;
        let (rl, ud) = (bp.apply(m, n.rl), bp.apply(m, n.ud));
        let (d, closer) = if rem <= NEAR {
            // Close to the goal the small in-cache table answers exactly.
            match self.near.get(rl, ud) {
                Some((d, closer)) if d as usize <= rem => (d, closer),
                _ => return None,
            }
        } else {
            // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
            let v = self.ctab.get(self.ctab.index(&self.tabs.ranks, rl, ud));
            let d = n.d + (v + 3 - (n.d + 2) % 3) % 3 - 1;
            if d as usize > rem {
                return None;
            }
            (d, UNKNOWN)
        };
        Some(Node {
            rl,
            ud,
            r: bp.apply(m, n.r),
            parity: n.parity ^ p2_flips_parity(m) as u8,
            d,
            closer,
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
        let share = std::env::var_os("CUBE444_NO_SHARE").is_none();
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
        // Development aid: stop after a given depth.
        let max_depth: Option<usize> =
            std::env::var("CUBE444_JOINT_MAXDEPTH").ok().and_then(|s| s.parse().ok());
        let have_endpoint = AtomicBool::new(false);
        let shared = Shared {
            count: AtomicUsize::new(0),
            limit: p.n2,
            budget: AtomicI64::new(p.joint_nodes),
            stop: AtomicBool::new(false),
            have_endpoint: &have_endpoint,
            deadline,
        };
        let mut found: Vec<P2Cand> = Vec::new();
        for depth in lower..=24 {
            let parts: Vec<Vec<P2Cand>> = roots
                .par_iter()
                .enumerate()
                .filter(|(_, root)| root.d as usize <= depth)
                .map(|(vi, root)| self.joint_par(&shared, variants, &partners[vi], vi, root, depth, &[]))
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
        // With no move to spare, only the moves that bring the centres closer can work.
        let allowed = if n.d as usize == rem { n.closer } else { UNKNOWN };
        for &m in &P1_MOVES {
            if (allowed >> m) & 1 == 0 || (last != 255 && skip_move(last, m)) {
                continue;
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
        // The wing parity is the same in every frame.
        if n.parity != 0 {
            return;
        }
        let solver = self.solver;
        if solver.rl_ok[(n.r >> 16) as usize] {
            let moves = self.path[..depth].to_vec();
            self.endpoint(self.variant, moves);
        }
        let partners = self.partners;
        for (k, part) in partners.iter().enumerate() {
            if part.ok[((n.pr[k] >> part.shift) & 255) as usize] {
                let moves = self.path[..depth].iter().map(|&m| part.relabel[m as usize]).collect();
                self.endpoint(part.variant, moves);
            }
        }
    }

    fn endpoint(&mut self, variant: usize, moves: Vec<u8>) {
        let tabs = &self.solver.tabs;
        let mut cube = self.variants[variant].cube;
        cube.apply_seq(&moves);
        if !edges_pairable(&cube) {
            return;
        }
        debug_assert_eq!(tabs.p2[tabs.p2_index(p2_mask(&cube), cube.wing_parity())], 0);
        let dc = tabs.ct3[tabs.ct3_index(ct3_mask(&cube), ct3_parity(&cube))];
        if dc == UNREACHED {
            return;
        }
        let ds = tabs.sigma[sigma_rank(&sigma_of(&cube))];
        let len = moves.len() as u8;
        self.out.push(P2Cand { variant, len1: len, moves, cube, len12: len, value: len + dc.max(ds) });
        self.shared.have_endpoint.store(true, Relaxed);
        if self.shared.count.fetch_add(1, Relaxed) + 1 >= self.shared.limit {
            self.shared.stop.store(true, Relaxed);
        }
    }
}
