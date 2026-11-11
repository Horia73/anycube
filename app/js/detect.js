// Finding a face of the cube in a picture, wherever it is: its N by N
// stickers as a lattice of alike patches, the four corners of that lattice,
// and the colour in the middle of each sticker with the perspective undone.
//
// How: the picture is smoothed a little and cut into patches, the flat
// places between its edges (a sticker is one; the gaps round it are edges).
// Patches that are filled and about square are kept as possible stickers.
// Then a lattice is looked for among their middles: a patch and one of its
// nearest neighbours say "one step of the lattice is from here to there",
// the other patches are asked whether they sit on whole steps of it, and
// the lattice is fitted again to all that do, first as a parallelogram,
// then with perspective. Of its N by N windows the one with the most
// stickers is taken, fitted once more to its own stickers, and kept only
// if it looks like a face: stickers of one size right on their places,
// dark gaps between them, four sides like a square's, all in the picture.
//
// What it does not do: tell a face from the same face turned by a quarter
// (the caller's order of rows is "as the picture shows them"), find a face
// turned by more than 35 degrees or seen much from the side, or one whose
// stickers are not set apart by something darker or of another colour.
//
// No DOM here, so that the tests can load it under Node. A caller hands in
// a frame scaled down to about 320 pixels on its longer side (480 for the
// 6x6x6 and 7x7x7, whose stickers are small); more than that only costs time.

import { cellColour } from "./colour.js";

/**
 * The least `score` a face is returned with: findFace answers null below
 * it, so a caller has nothing to compare. A 2x2x2, with four stickers to
 * go by, needs 0.9.
 */
export const FACE_SCORE = 0.75;
// The same for a face with no dark gaps between its stickers.
const PALE_SCORE = 0.86;

// What counts as an edge between patches: a change, summed over the three
// channels across and down, of 3.2 times the picture's usual one, but no
// less and no more than these.
const EDGE_LEAST = 16;
const EDGE_MOST = 46;
// No more patches than this are tried as stickers.
const MOST_PATCHES = 150;
// The band along a patch's edges that belongs to the edge, not to the
// patch: a sticker is this much wider than its patch.
const BAND = 3;
// A gap is dark when it is no lighter than this share of the stickers on either side of it.
const DIP = 0.82;
// How far from a whole step of the lattice a patch's middle may be, in steps.
const NEAR = 0.3;
// And how far once the lattice has been fitted to the face's own stickers.
const EXACT = 0.13;

// ------------------------------------------------------------ geometry

// Solves A x = b for a small dense system, in place; null if it has no one answer.
function solve(A, b) {
  const n = b.length;
  for (let c = 0; c < n; c++) {
    let p = c;
    for (let r = c + 1; r < n; r++) if (Math.abs(A[r][c]) > Math.abs(A[p][c])) p = r;
    if (Math.abs(A[p][c]) < 1e-10) return null;
    [A[c], A[p]] = [A[p], A[c]];
    [b[c], b[p]] = [b[p], b[c]];
    for (let r = c + 1; r < n; r++) {
      const f = A[r][c] / A[c][c];
      if (f === 0) continue;
      for (let k = c; k < n; k++) A[r][k] -= f * A[c][k];
      b[r] -= f * b[c];
    }
  }
  const x = new Array(n);
  for (let r = n - 1; r >= 0; r--) {
    let s = b[r];
    for (let k = r + 1; k < n; k++) s -= A[r][k] * x[k];
    x[r] = s / A[r][r];
  }
  return x;
}

/**
 * The projective map that takes each point of `from` nearest to its
 * partner in `to` (least squares; four pairs or more, no three of the four
 * in a line), as nine numbers row by row with the last one 1; null if
 * the points do not settle one.
 */
export function solveHomography(from, to) {
  if (from.length < 4) return null;
  // The equations are kept well-conditioned by measuring `to` in units of its own size.
  let scale = 1;
  for (const [X, Y] of to) scale = Math.max(scale, Math.abs(X), Math.abs(Y));
  const A = Array.from({ length: 8 }, () => new Array(8).fill(0));
  const b = new Array(8).fill(0);
  const add = (row, value) => {
    for (let i = 0; i < 8; i++) {
      if (row[i] === 0) continue;
      b[i] += row[i] * value;
      for (let k = 0; k < 8; k++) A[i][k] += row[i] * row[k];
    }
  };
  for (let p = 0; p < from.length; p++) {
    const [x, y] = from[p];
    const [X, Y] = [to[p][0] / scale, to[p][1] / scale];
    add([x, y, 1, 0, 0, 0, -x * X, -y * X], X);
    add([0, 0, 0, x, y, 1, -x * Y, -y * Y], Y);
  }
  const h = solve(A, b);
  if (!h || !h.every(Number.isFinite)) return null;
  return [h[0] * scale, h[1] * scale, h[2] * scale, h[3] * scale, h[4] * scale, h[5] * scale, h[6], h[7], 1];
}

/** The same without perspective (three pairs or more, not in a line), in the same nine numbers. */
export function solveAffine(from, to) {
  if (from.length < 3) return null;
  const A = [
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
  ];
  const bx = [0, 0, 0];
  const by = [0, 0, 0];
  for (let p = 0; p < from.length; p++) {
    const row = [from[p][0], from[p][1], 1];
    for (let i = 0; i < 3; i++) {
      bx[i] += row[i] * to[p][0];
      by[i] += row[i] * to[p][1];
      for (let k = 0; k < 3; k++) A[i][k] += row[i] * row[k];
    }
  }
  const x = solve(A.map((r) => r.slice()), bx);
  const y = solve(A, by);
  if (!x || !y) return null;
  return [x[0], x[1], x[2], y[0], y[1], y[2], 0, 0, 1];
}

/** Where the map `h` takes the point (x, y). */
export function warpPoint(h, x, y) {
  const w = h[6] * x + h[7] * y + h[8];
  return [(h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w];
}

/** The map that undoes `h`; null if nothing does. */
export function invertHomography(h) {
  const [a, b, c, d, e, f, g, i, j] = h;
  const A = e * j - f * i;
  const B = f * g - d * j;
  const C = d * i - e * g;
  const det = a * A + b * B + c * C;
  if (Math.abs(det) < 1e-12) return null;
  return [A / det, (c * i - b * j) / det, (b * f - c * e) / det, B / det, (a * j - c * g) / det, (c * d - a * f) / det, C / det, (b * g - a * i) / det, (a * e - b * d) / det];
}

// ------------------------------------------------------------- patches

// The picture cut into patches: the flat places between its edges.
// Returns, for every pixel, the pixel that stands for its patch, and for
// those pixels the patch's size; the patches that could be stickers, with
// their middles; and the smoothed picture.
function patches(image, n) {
  const { width: W, height: H, data } = image;
  const N = W * H;
  // Smoothed over three by three, which takes most of a camera's noise out
  // and leaves a gap of one pixel still a third as deep as it was.
  const sum = [new Uint16Array(N), new Uint16Array(N), new Uint16Array(N)];
  const row = new Uint16Array(W);
  for (let ch = 0; ch < 3; ch++) {
    const out = sum[ch];
    for (let y = 0; y < H; y++) {
      const base = y * W;
      for (let x = 0; x < W; x++) {
        const l = x > 0 ? x - 1 : 0;
        const r = x < W - 1 ? x + 1 : x;
        row[x] = data[(base + l) * 4 + ch] + data[(base + x) * 4 + ch] + data[(base + r) * 4 + ch];
      }
      for (let x = 0; x < W; x++) out[base + x] = row[x];
    }
    // Down the columns, in place from a copy of each column's three rows.
    const above = new Uint16Array(W);
    const here = new Uint16Array(W);
    for (let x = 0; x < W; x++) above[x] = here[x] = out[x];
    for (let y = 0; y < H; y++) {
      const base = y * W;
      const next = y < H - 1 ? base + W : base;
      for (let x = 0; x < W; x++) {
        const below = out[next + x];
        const v = above[x] + here[x] + below;
        above[x] = here[x];
        here[x] = below;
        out[base + x] = v;
      }
    }
  }
  const [R, G, B] = sum; // nine times the smoothed value

  // Where the picture changes: how much it differs two pixels apart, across
  // and down, summed over the channels. An edge is where that is well above
  // what the picture's own noise makes of a flat place, which is what most
  // of any picture is. (Growing patches from pixel to alike pixel instead
  // does not hold: along an edge that is a little off the pixels' rows,
  // each step is small, and a sticker runs out into the body round it.)
  const change = new Uint16Array(N);
  const counts = new Uint32Array(256);
  for (let y = 0; y < H; y++) {
    const base = y * W;
    const up = y > 0 ? base - W : base;
    const dn = y < H - 1 ? base + W : base;
    for (let x = 0; x < W; x++) {
      const l = base + (x > 0 ? x - 1 : 0);
      const r = base + (x < W - 1 ? x + 1 : x);
      const g =
        Math.abs(R[r] - R[l]) + Math.abs(G[r] - G[l]) + Math.abs(B[r] - B[l]) + Math.abs(R[dn + x] - R[up + x]) + Math.abs(G[dn + x] - G[up + x]) + Math.abs(B[dn + x] - B[up + x]);
      change[base + x] = g;
      counts[Math.min(255, Math.round(g / 9))]++;
    }
  }
  let median = 0;
  for (let seen = 0; median < 255; median++) {
    seen += counts[median];
    if (seen * 2 >= N) break;
  }
  const edge = Math.max(EDGE_LEAST, Math.min(EDGE_MOST, 3.2 * median)) * 9;

  // Neighbours that are not on an edge are of one patch.
  const parent = new Int32Array(N);
  for (let i = 0; i < N; i++) parent[i] = i;
  for (let y = 0, i = 0; y < H; y++) {
    for (let x = 0; x < W; x++, i++) {
      if (change[i] > edge) continue;
      let p = i;
      if (x > 0 && change[i - 1] <= edge) {
        // The pixel to the left has been through here: what stands for it stands for this one.
        p = i - 1;
        while (parent[p] !== p) p = parent[p] = parent[parent[p]];
        parent[i] = p;
      }
      if (y > 0 && change[i - W] <= edge) {
        let q = i - W;
        while (parent[q] !== q) q = parent[q] = parent[parent[q]];
        if (q !== p) parent[q > p ? q : p] = q > p ? p : q;
      }
    }
  }
  const size = new Int32Array(N);
  for (let i = 0; i < N; i++) {
    let p = parent[i];
    while (parent[p] !== p) p = parent[p];
    parent[i] = p;
    size[p]++;
  }

  // A sticker's patch, from a face a fifth of the picture's shorter side
  // across to one that fills it.
  const shorter = Math.min(W, H);
  const least = Math.max(12, ((0.15 * shorter) / n - BAND) ** 2);
  const most = (shorter / n) ** 2;
  const slot = new Int32Array(N).fill(-1);
  let count = 0;
  for (let i = 0; i < N; i++) {
    if (parent[i] === i && size[i] >= least && size[i] <= most) slot[i] = count++;
  }
  // Where each such patch is and how it is spread, from sums over its pixels.
  const sums = new Float64Array(count * 5);
  for (let y = 0, i = 0; y < H; y++) {
    for (let x = 0; x < W; x++, i++) {
      const s = slot[parent[i]];
      if (s < 0) continue;
      const k = s * 5;
      sums[k] += x;
      sums[k + 1] += y;
      sums[k + 2] += x * x;
      sums[k + 3] += y * y;
      sums[k + 4] += x * y;
    }
  }
  const stickers = [];
  for (let i = 0; i < N; i++) {
    const s = slot[i];
    if (s < 0) continue;
    const a = size[i];
    const k = s * 5;
    const cx = sums[k] / a;
    const cy = sums[k + 1] / a;
    const vxx = sums[k + 2] / a - cx * cx + 1 / 12;
    const vyy = sums[k + 3] / a - cy * cy + 1 / 12;
    const vxy = sums[k + 4] / a - cx * cy;
    const det = vxx * vyy - vxy * vxy;
    if (det <= 0) continue;
    // A filled parallelogram has an area of twelve times the root of this;
    // an L, a frame or a ragged blot has less.
    const filled = a / (12 * Math.sqrt(det));
    if (filled < 0.72 || filled > 1.25) continue;
    const tr = vxx + vyy;
    const spread = Math.sqrt(Math.max(0, tr * tr - 4 * det));
    // No longer than twice as wide.
    if ((tr + spread) / Math.max(1e-9, tr - spread) > 4) continue;
    stickers.push({ x: cx, y: cy, side: Math.sqrt(a) + BAND, area: a });
  }
  // A picture full of small things has hundreds; the largest are the ones a face would be.
  if (stickers.length > MOST_PATCHES) {
    stickers.sort((p, q) => q.area - p.area);
    stickers.length = MOST_PATCHES;
  }
  return { parent, size, stickers, R, G, B };
}

// ------------------------------------------------------------- lattice

// The stickers that sit on whole steps of the lattice `h` (lattice to
// picture), no further than `reach` steps from its origin: one per place,
// the nearest.
function settle(h, stickers, side, reach) {
  const back = invertHomography(h);
  if (!back) return [];
  const best = new Map();
  for (let k = 0; k < stickers.length; k++) {
    const s = stickers[k];
    if (s.side < side * 0.6 || s.side > side * 1.67) continue;
    const w = back[6] * s.x + back[7] * s.y + back[8];
    const i = (back[0] * s.x + back[1] * s.y + back[2]) / w;
    const j = (back[3] * s.x + back[4] * s.y + back[5]) / w;
    const ri = Math.round(i);
    const rj = Math.round(j);
    if (Math.abs(ri) > reach || Math.abs(rj) > reach) continue;
    const off = Math.max(Math.abs(i - ri), Math.abs(j - rj));
    if (!(off <= NEAR)) continue;
    const key = (ri + 16) * 64 + (rj + 16);
    const was = best.get(key);
    if (!was || off < was.off) best.set(key, { i: ri, j: rj, k, off });
  }
  return [...best.values()];
}

// The lattice through one sticker with one step given, fitted to all the
// stickers that agree with it: the map and those stickers, or null.
function grow(stickers, a, ux, uy, n) {
  const o = stickers[a];
  let h = [ux, -uy, o.x, uy, ux, o.y, 0, 0, 1];
  let on = [];
  // Outwards a little at a time: a step measured between two neighbours is
  // not exact enough to be trusted far away.
  const reaches = n > 3 ? [1, 2, n - 1, n - 1, n - 1] : [1, n - 1, n - 1, n - 1];
  const half = Math.ceil((n * n) / 2);
  for (let pass = 0; pass < reaches.length; pass++) {
    on = settle(h, stickers, o.side, reaches[pass]);
    // Three to fit a lattice to at all; and once all of it is in reach, half a face.
    if (on.length < 3 || (reaches[pass] === n - 1 && on.length < half)) return null;
    const from = on.map((c) => [c.i, c.j]);
    const to = on.map((c) => [stickers[c.k].x, stickers[c.k].y]);
    const next = model(from, to, pass >= 2, n);
    if (!next) return null;
    h = next;
  }
  on = settle(h, stickers, o.side, n - 1);
  return on.length >= half ? { h, on } : null;
}

// The lattice through these stickers. Perspective only once there is
// enough to hold it down: six stickers, or the four of a 2x2x2's face.
function model(from, to, projective, n) {
  const enough = from.length >= 6 || (n === 2 && from.length === 4);
  return (projective && enough && solveHomography(from, to)) || solveAffine(from, to);
}

// Whether a lattice looks like a face held up to a camera: its two steps
// about as long as each other and about square to each other, one of them
// running across the picture and the other down it (a face turned by more
// than 35 degrees is not told from one turned the other way), and its
// stickers filling most of a step.
function plausible(h, i, j, side) {
  const at = warpPoint(h, i, j);
  const a = warpPoint(h, i + 1, j);
  const b = warpPoint(h, i, j + 1);
  const [ux, uy, vx, vy] = [a[0] - at[0], a[1] - at[1], b[0] - at[0], b[1] - at[1]];
  const [lu, lv] = [Math.hypot(ux, uy), Math.hypot(vx, vy)];
  if (lu < lv * 0.72 || lv < lu * 0.72) return false;
  if (Math.abs(ux * vx + uy * vy) > 0.42 * lu * lv) return false; // further than 25 degrees from square
  const [across, down] = Math.abs(ux) >= Math.abs(vx) ? [Math.abs(uy) / lu, Math.abs(vx) / lv] : [Math.abs(vy) / lv, Math.abs(ux) / lu];
  if (across > 0.574 || down > 0.574) return false; // the sine of 35 degrees
  const step = Math.sqrt(lu * lv);
  return side > step * 0.52 && side < step * 1.08;
}

// The N by N windows of a lattice that could hold the face: those with the
// most stickers in them, the fullest first.
function windowsOf(on, n) {
  let [i0, i1, j0, j1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const c of on) {
    i0 = Math.min(i0, c.i);
    i1 = Math.max(i1, c.i);
    j0 = Math.min(j0, c.j);
    j1 = Math.max(j1, c.j);
  }
  const windows = [];
  for (let a = i0 - n + 1; a <= i1; a++) {
    for (let b = j0 - n + 1; b <= j1; b++) {
      let held = 0;
      for (const c of on) if (c.i >= a && c.i < a + n && c.j >= b && c.j < b + n) held++;
      windows.push({ a, b, held });
    }
  }
  windows.sort((p, q) => q.held - p.held);
  return windows.filter((w) => w.held >= windows[0].held - 1).slice(0, 3);
}

// A lattice and one of its windows, looked at closely: the lattice fitted
// again to the stickers of the window alone (one beside the face that
// happened to sit on the lattice has pulled at it till then), and then
// only what a face would show counted: stickers right on their places,
// all of a size, filling most of a step, on steps that are about square.
// Returns the lattice, the window and its score, or null.
function rate(fit, w, cut, image, n) {
  const { parent, size, stickers } = cut;
  const { width: W, height: H } = image;
  const { a, b } = w;
  const ref = stickers[fit.on[0].k].side;
  const within = (c) => c.i >= a && c.i < a + n && c.j >= b && c.j < b + n;
  let h = fit.h;
  let inside = [];
  for (let pass = 0; pass < 3; pass++) {
    inside = settle(h, stickers, ref, 2 * n).filter((c) => within(c) && c.off <= (pass < 2 ? 0.22 : EXACT));
    if (inside.length < 3) return null;
    if (pass === 2) break;
    const next = model(
      inside.map((c) => [c.i, c.j]),
      inside.map((c) => [stickers[c.k].x, stickers[c.k].y]),
      true,
      n,
    );
    if (!next) return null;
    h = next;
  }
  // All of a size.
  const sides = inside.map((c) => stickers[c.k].side).sort((p, q) => p - q);
  const side = sides[sides.length >> 1];
  inside = inside.filter((c) => stickers[c.k].side >= side * 0.8 && stickers[c.k].side <= side * 1.25);
  if (inside.length < 3) return null;
  if (!plausible(h, a + (n - 1) / 2, b + (n - 1) / 2, side)) return null;
  // All of the face is in the picture, and its four sides make something
  // like a square seen from near the front: opposite sides within a third
  // of each other, no corner sharper than 60 degrees.
  const q = [
    [a - 0.5, b - 0.5],
    [a + n - 0.5, b - 0.5],
    [a + n - 0.5, b + n - 0.5],
    [a - 0.5, b + n - 0.5],
  ].map(([i, j]) => warpPoint(h, i, j));
  const edges = q.map(([x, y], k) => [q[(k + 1) % 4][0] - x, q[(k + 1) % 4][1] - y]);
  const lengths = edges.map(([x, y]) => Math.hypot(x, y));
  for (let k = 0; k < 4; k++) {
    const [x, y] = q[k];
    if (!(x >= -side * 0.15 && y >= -side * 0.15 && x <= W - 1 + side * 0.15 && y <= H - 1 + side * 0.15)) return null;
    if (lengths[k] < 0.67 * lengths[(k + 2) % 4] || lengths[k] < 0.6 * lengths[(k + 1) % 4]) return null;
    const next = edges[(k + 1) % 4];
    if (Math.abs(edges[k][0] * next[0] + edges[k][1] * next[1]) > 0.5 * lengths[k] * lengths[(k + 1) % 4]) return null;
  }
  // Four points always lie on some lattice seen in perspective: the four
  // stickers of a 2x2x2 have to lie near one seen flat as well.
  if (n === 2) {
    const flat = solveAffine(
      inside.map((c) => [c.i, c.j]),
      inside.map((c) => [stickers[c.k].x, stickers[c.k].y]),
    );
    const back = flat && invertHomography(flat);
    if (!back) return null;
    for (const c of inside) {
      const [i, j] = warpPoint(back, stickers[c.k].x, stickers[c.k].y);
      if (Math.max(Math.abs(i - c.i), Math.abs(j - c.j)) > 0.1) return null;
    }
  }

  // Places with no sticker told apart, by the patch their middle lies in.
  const taken = new Set(inside.map((c) => c.i * 64 + c.j));
  const shared = new Map();
  for (let i = a; i < a + n; i++) {
    for (let j = b; j < b + n; j++) {
      if (taken.has(i * 64 + j)) continue;
      const [x, y] = warpPoint(h, i, j);
      const [px, py] = [Math.round(x), Math.round(y)];
      if (px < 0 || py < 0 || px >= W || py >= H) continue;
      const root = parent[py * W + px];
      shared.set(root, (shared.get(root) ?? 0) + 1);
    }
  }
  // Stickers of one colour side by side run together into one patch: two
  // places or more in a patch about as large as that many stickers, lying
  // inside the window.
  let likely = 0;
  const back = invertHomography(h);
  const areas = inside.map((c) => stickers[c.k].area).sort((p, q) => p - q);
  const usual = areas[areas.length >> 1];
  for (const [root, places] of shared) {
    if (places < 2 || size[root] < usual * 0.6 * places || size[root] > usual * 1.7 * places) continue;
    const [ri, rj] = back ? warpPoint(back, root % W, Math.floor(root / W)) : [NaN, NaN];
    if (!(ri >= a - 0.6 && ri <= a + n - 0.4 && rj >= b - 0.6 && rj <= b + n - 0.4)) continue;
    likely += places;
  }
  // What ran together only adds to what was told apart: six tenths of the face have to be that.
  if (inside.length < 0.6 * n * n) return null;
  const score = (inside.length + 0.8 * likely) / (n * n);

  // Between two stickers of a face lies the cube's body, darker than they
  // are: a wide gap or a seam of a pixel, which after smoothing is still a
  // dip between them. Squares that only happen to stand in rows (tiles,
  // keys, books) have anything between them. Where the gaps are not dark
  // (a cube with a pale body), nearly all of the face has to have been
  // seen instead; the four stickers of a 2x2x2 are too few to do without.
  const { R, G, B } = cut;
  const light = (i, j) => {
    const [x, y] = warpPoint(h, i, j);
    const [px, py] = [Math.round(x), Math.round(y)];
    if (px < 0 || py < 0 || px >= W || py >= H) return -1;
    const at = py * W + px;
    return R[at] + G[at] + B[at];
  };
  let [gaps, dark] = [0, 0];
  for (const c of inside) {
    for (const [di, dj] of [
      [1, 0],
      [0, 1],
    ]) {
      if (!taken.has((c.i + di) * 64 + c.j + dj)) continue;
      // The darkest of a few points across the middle of the step.
      let between = Infinity;
      for (const t of [0.4, 0.45, 0.5, 0.55, 0.6]) {
        const l = light(c.i + di * t, c.j + dj * t);
        if (l >= 0 && l < between) between = l;
      }
      if (between === Infinity) continue;
      gaps++;
      if (between < DIP * 0.5 * (light(c.i, c.j) + light(c.i + di, c.j + dj))) dark++;
    }
    // And where four stickers meet.
    if (taken.has((c.i + 1) * 64 + c.j) && taken.has(c.i * 64 + c.j + 1) && taken.has((c.i + 1) * 64 + c.j + 1)) {
      const between = light(c.i + 0.5, c.j + 0.5);
      if (between >= 0) {
        gaps++;
        if (between < DIP * 0.25 * (light(c.i, c.j) + light(c.i + 1, c.j) + light(c.i, c.j + 1) + light(c.i + 1, c.j + 1))) dark++;
      }
    }
  }
  const bodied = gaps > 0 && dark >= (n === 2 ? 1 : 0.75) * gaps;
  if (score < (n === 2 ? 0.9 : bodied ? FACE_SCORE : PALE_SCORE) || (n === 2 && !bodied)) return null;
  let off = 0;
  for (const c of inside) off += c.off;
  return { h, a, b, score, mean: off / inside.length };
}

// ---------------------------------------------------------------- face

/**
 * Finds a face of the cube in a picture.
 *
 * @param image `{ width, height, data }`, RGBA bytes, as an ImageData has them
 * @param {number} n stickers along one side of the face, 2 to 7
 * @returns null when no face is found, else `corners` (the four outer
 *   corners of the N by N stickers, in the picture's pixels: top left, top
 *   right, bottom right, bottom left as the picture shows them), `cells`
 *   (N*N colours `[r, g, b]`, row by row from the top left, each the middle
 *   of its sticker) and `score` (0 to 1: the share of the N*N places of the
 *   lattice where a sticker was seen).
 */
export function findFace(image, n) {
  const { width: W, height: H, data } = image;
  if (!W || !H || n < 2) return null;
  const cut = patches(image, n);
  const { stickers } = cut;
  if (stickers.length < 3) return null;

  let best = null;
  const tried = new Set();
  search: for (let s0 = 0; s0 < stickers.length; s0++) {
    const s = stickers[s0];
    // A step of the lattice: from this sticker to one of the nearest three of its size.
    const near = [];
    for (let s1 = 0; s1 < stickers.length; s1++) {
      if (s1 === s0) continue;
      const t = stickers[s1];
      if (t.side < s.side * 0.75 || t.side > s.side * 1.33) continue;
      const d = Math.hypot(t.x - s.x, t.y - s.y);
      const mean = (s.side + t.side) / 2;
      if (d < mean * 0.9 || d > mean * 2.2) continue;
      near.push({ s1, d });
    }
    near.sort((p, q) => p.d - q.d);
    for (const { s1 } of near.slice(0, 3)) {
      const t = stickers[s1];
      const fit = grow(stickers, s0, t.x - s.x, t.y - s.y, n);
      if (!fit) continue;
      // The same stickers found again say nothing new.
      const key = fit.on
        .map((c) => c.k)
        .sort((p, q) => p - q)
        .join(",");
      if (tried.has(key)) continue;
      tried.add(key);
      for (const w of windowsOf(fit.on, n)) {
        if (w.held < 3) break;
        const rated = rate(fit, w, cut, image, n);
        if (!rated) continue;
        // The fullest; of two as full, the one its stickers sit on most exactly
        // (stickers beside a face that happen to line up with it do so loosely).
        if (!best || rated.score > best.score + 1e-9 || (Math.abs(rated.score - best.score) <= 1e-9 && rated.mean < best.mean)) best = rated;
        // Full and exact: there is nothing better to look for.
        if (best.score >= 0.999 && best.mean < EXACT / 4) break search;
      }
    }
  }
  if (!best) return null;
  const { h, a, b } = best;

  // The lattice says nothing of which way is up: rows run down the picture
  // and columns across it, by whichever of its two directions is nearer.
  const mid = [a + (n - 1) / 2, b + (n - 1) / 2];
  const at = warpPoint(h, mid[0], mid[1]);
  const di = warpPoint(h, mid[0] + 1, mid[1]);
  const dj = warpPoint(h, mid[0], mid[1] + 1);
  const [ui, uj] = [
    [di[0] - at[0], di[1] - at[1]],
    [dj[0] - at[0], dj[1] - at[1]],
  ];
  const across = Math.abs(ui[0]) >= Math.abs(uj[0]); // columns run along i
  const right = (across ? ui[0] : uj[0]) >= 0;
  const down = (across ? uj[1] : ui[1]) >= 0;
  // A point of the face, `col` and `row` each from 0 to n across it, in the picture.
  const place = (col, row) => {
    const c = right ? col - 0.5 : n - 0.5 - col;
    const r = down ? row - 0.5 : n - 0.5 - row;
    return across ? warpPoint(h, a + c, b + r) : warpPoint(h, a + r, b + c);
  };
  const corners = [place(0, 0), place(n, 0), place(n, n), place(0, n)];

  // The middle half of each sticker, read from the picture as it came.
  const M = 5;
  const cells = [];
  const buffer = new Uint8ClampedArray(M * M * 4);
  for (let row = 0; row < n; row++) {
    for (let col = 0; col < n; col++) {
      let k = 0;
      for (let v = 0; v < M; v++) {
        for (let u = 0; u < M; u++) {
          const [x, y] = place(col + 0.25 + (0.5 * (u + 0.5)) / M, row + 0.25 + (0.5 * (v + 0.5)) / M);
          const px = Math.max(0, Math.min(W - 1, Math.round(x)));
          const py = Math.max(0, Math.min(H - 1, Math.round(y)));
          const i = (py * W + px) * 4;
          buffer[k++] = data[i];
          buffer[k++] = data[i + 1];
          buffer[k++] = data[i + 2];
          buffer[k++] = 255;
        }
      }
      cells.push(cellColour(buffer));
    }
  }
  return { corners, cells, score: Math.min(1, best.score) };
}
