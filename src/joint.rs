//! Phases 1 and 2 as one search: from any state, stage all centres, bring
//! the R/L centres into a finishable pattern, make the wing permutation even
//! and the dedges pairable, using every move.
//!
//! The exact centre table makes the centre part of the search perfect: only
//! sequences that really stage the centres in the given number of moves are
//! followed. The other conditions are tested at the end.

use crate::ctab::NEAR;
use crate::cube::{skip_move, Cube};
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
}

const UNKNOWN: u32 = u32::MAX;

struct Shared<'a> {
    count: AtomicUsize,
    limit: usize,
    budget: AtomicI64,
    stop: AtomicBool,
    /// The node budget only applies once this is true.
    have_endpoint: &'a AtomicBool,
}

struct Ctx<'a> {
    solver: &'a Solver,
    shared: &'a Shared<'a>,
    variant: usize,
    root: Cube,
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
        Node { rl: m[2], ud: m[0], r, parity: cube.wing_parity(), d, closer: UNKNOWN }
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
        Some(Node { rl, ud, r: bp.apply(m, n.r), parity: n.parity ^ p2_flips_parity(m) as u8, d, closer })
    }

    /// Endpoints of the joint phase, by increasing length, until `p.n2` are found
    /// or the node budget is spent.
    pub(crate) fn joint_endpoints(&self, variants: &[Variant], p: &Params) -> Result<Vec<P2Cand>, String> {
        let roots: Vec<Node> = variants.iter().map(|v| self.joint_root(&v.cube)).collect();
        let lower = roots.iter().map(|n| n.d as usize).min().unwrap();
        let have_endpoint = AtomicBool::new(false);
        let shared = Shared {
            count: AtomicUsize::new(0),
            limit: p.n2,
            budget: AtomicI64::new(p.joint_nodes),
            stop: AtomicBool::new(false),
            have_endpoint: &have_endpoint,
        };
        let mut found: Vec<P2Cand> = Vec::new();
        for depth in lower..=24 {
            // Split every variant's search two moves deep and run the pieces in parallel.
            let mut jobs: Vec<(usize, Vec<u8>, Node)> = Vec::new();
            for (vi, root) in roots.iter().enumerate() {
                if root.d as usize > depth {
                    continue;
                }
                let split = depth.min(2);
                let mut level = vec![(Vec::new(), *root)];
                for s in 0..split {
                    let mut next = Vec::new();
                    for (path, node) in &level {
                        let last = path.last().copied().unwrap_or(255);
                        for &m in &P1_MOVES {
                            if last != 255 && skip_move(last, m) {
                                continue;
                            }
                            if let Some(n) = self.joint_step(node, m, depth - s - 1) {
                                let mut q = path.clone();
                                q.push(m);
                                next.push((q, n));
                            }
                        }
                    }
                    level = next;
                }
                jobs.extend(level.into_iter().map(|(path, node)| (vi, path, node)));
            }
            let parts: Vec<Vec<P2Cand>> = jobs
                .par_iter()
                .map(|(vi, prefix, node)| {
                    let mut ctx = Ctx {
                        solver: self,
                        shared: &shared,
                        variant: *vi,
                        root: variants[*vi].cube,
                        path: [0; 32],
                        out: Vec::new(),
                        nodes: 0,
                    };
                    ctx.path[..prefix.len()].copy_from_slice(prefix);
                    let last = prefix.last().copied().unwrap_or(255);
                    ctx.dfs(node, depth - prefix.len(), last, prefix.len());
                    ctx.out
                })
                .collect();
            found.extend(parts.into_iter().flatten());
            if !found.is_empty() {
                have_endpoint.store(true, Relaxed);
            }
            if found.len() >= p.n2 || (!found.is_empty() && shared.budget.load(Relaxed) < 0) {
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
            if left < 0 && self.shared.have_endpoint.load(Relaxed) {
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

    /// The centres are staged; test everything else phase 3 needs.
    fn leaf(&mut self, n: &Node, depth: usize) {
        if n.parity != 0 || !self.solver.rl_ok[(n.r >> 16) as usize] {
            return;
        }
        let tabs = &self.solver.tabs;
        let mut cube = self.root;
        cube.apply_seq(&self.path[..depth]);
        if !edges_pairable(&cube) {
            return;
        }
        let dc = tabs.ct3[tabs.ct3_index(ct3_mask(&cube), ct3_parity(&cube))];
        if dc == UNREACHED {
            return;
        }
        let ds = tabs.sigma[sigma_rank(&sigma_of(&cube))];
        self.out.push(P2Cand {
            variant: self.variant,
            len1: depth as u8,
            moves: self.path[..depth].to_vec(),
            cube,
            len12: depth as u8,
            value: depth as u8 + dc.max(ds),
        });
        self.shared.have_endpoint.store(true, Relaxed);
        if self.shared.count.fetch_add(1, Relaxed) + 1 >= self.shared.limit {
            self.shared.stop.store(true, Relaxed);
        }
    }
}
