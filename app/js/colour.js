// From the colours a camera saw on the six faces to the six colours of a cube.
//
// Nothing here knows what "red" looks like. The 6*N*N samples are sorted
// into six groups of N*N each by how alike they are, starting from the
// middle sticker of each face (which on a cube of odd size is that face's
// colour by definition), so the light in the room, the camera's white
// balance and the shade of a particular cube's stickers matter little:
// what counts is that a cube's reds are more like each other than like its
// oranges. A cube of even size has no middle stickers; its groups start
// from the six samples furthest apart, and a corner says which group
// belongs on which face.
//
// No DOM here, so that the tests can load it under Node.

import { CubeModel } from "./cube-model.js";

// The page's sticker colours, in face order U R F D L B.
export const STICKERS = ["#f4f4ef", "#d92b3a", "#1faa59", "#f7d038", "#f58220", "#2f6fde"];

// How much lightness counts next to hue. The same sticker is lighter on one
// face than on another, as each was held to the light differently, so
// samples are compared by what stays the same when the light changes: the
// two colour axes of CIELAB divided by the lightness (all three grow with
// the cube root of the light, so the ratios do not move). Lightness itself
// is kept with a small weight, to tell white from the rest when colours are pale.
const LIGHTNESS = 0.25;

function toLab([r, g, b]) {
  const lin = (v) => {
    v /= 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  const [R, G, B] = [lin(r), lin(g), lin(b)];
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  const x = f((0.4124 * R + 0.3576 * G + 0.1805 * B) / 0.95047);
  const y = f(0.2126 * R + 0.7152 * G + 0.0722 * B);
  const z = f((0.0193 * R + 0.1192 * G + 0.9505 * B) / 1.08883);
  return [116 * y - 16, 500 * (x - y), 200 * (y - z)];
}

// A colour as the three numbers samples are compared by. Dividing by the
// lightness alone would make the hue of a dark sample (a black sticker, a
// face in shadow) a matter of noise; the constant added to it trades a
// little of the independence from light for that: measured on made-up
// pictures, black stickers go wrong in half the cubes without it and in
// one in forty with it.
export function feature(rgb) {
  const [L, a, b] = toLab(rgb);
  return [L * LIGHTNESS, (100 * a) / (L + 60), (100 * b) / (L + 60)];
}

function distance(p, q) {
  return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]);
}

function hexToRgb(hex) {
  const v = parseInt(hex.slice(1), 16);
  return [v >> 16, (v >> 8) & 255, v & 255];
}

// The assignment of six things to six others that costs least.
function bestMatch(cost) {
  let best = null;
  let bestCost = Infinity;
  const perm = [];
  const used = new Array(6).fill(false);
  const go = (i, total) => {
    if (total >= bestCost) return;
    if (i === 6) {
      bestCost = total;
      best = perm.slice();
      return;
    }
    for (let k = 0; k < 6; k++) {
      if (used[k]) continue;
      used[k] = true;
      perm[i] = k;
      go(i + 1, total + cost[i][k]);
      used[k] = false;
    }
  };
  go(0, 0);
  return best;
}

// Sorts the samples into six groups of equal size. `seeds` are six sample
// indices to start the groups from; `pinned` says whether a seed stays in
// its own group whatever happens (the middle stickers of an odd cube).
function group(lab, seeds, pinned) {
  const total = lab.length;
  const each = total / 6;
  let centres = seeds.map((i) => lab[i]);
  let labels = new Int8Array(total).fill(-1);
  const fixed = new Int8Array(total).fill(-1);
  if (pinned) seeds.forEach((i, k) => (fixed[i] = k));
  let cost = [];
  for (let round = 0; round < 16; round++) {
    cost = lab.map((p) => centres.map((c) => distance(p, c)));
    const next = new Int8Array(total).fill(-1);
    const room = new Array(6).fill(each);
    for (let i = 0; i < total; i++) {
      if (fixed[i] >= 0) {
        next[i] = fixed[i];
        room[fixed[i]]--;
      }
    }
    // Those with most to lose by not getting their first choice choose first.
    const order = [];
    for (let i = 0; i < total; i++) {
      if (next[i] >= 0) continue;
      const sorted = cost[i].slice().sort((a, b) => a - b);
      order.push([sorted[1] - sorted[0], i]);
    }
    order.sort((a, b) => b[0] - a[0]);
    for (const [, i] of order) {
      let k = -1;
      for (let c = 0; c < 6; c++) if (room[c] > 0 && (k < 0 || cost[i][c] < cost[i][k])) k = c;
      next[i] = k;
      room[k]--;
    }
    // Then any two that would both be better off, or better off together, change places.
    for (let improved = true, pass = 0; improved && pass < 20; pass++) {
      improved = false;
      for (let i = 0; i < total; i++) {
        if (fixed[i] >= 0) continue;
        for (let j = i + 1; j < total; j++) {
          if (fixed[j] >= 0 || next[i] === next[j]) continue;
          if (cost[i][next[j]] + cost[j][next[i]] < cost[i][next[i]] + cost[j][next[j]] - 1e-9) {
            [next[i], next[j]] = [next[j], next[i]];
            improved = true;
          }
        }
      }
    }
    const same = next.every((v, i) => v === labels[i]);
    labels = next;
    if (same) break;
    centres = centres.map((_, k) => {
      const sum = [0, 0, 0];
      for (let i = 0; i < total; i++) if (labels[i] === k) for (let d = 0; d < 3; d++) sum[d] += lab[i][d];
      return sum.map((v) => v / each);
    });
  }
  // How unsure each sample is: 0 when its group is far nearer than any other, 1 when two are as near.
  const doubt = new Float32Array(total);
  for (let i = 0; i < total; i++) {
    const own = cost[i][labels[i]];
    let other = Infinity;
    for (let c = 0; c < 6; c++) if (c !== labels[i]) other = Math.min(other, cost[i][c]);
    doubt[i] = Math.max(0, Math.min(1, own / (other + 1e-6)));
  }
  let spread = 0;
  for (let i = 0; i < total; i++) spread += cost[i][labels[i]];
  return { labels, doubt, spread };
}

// The eight corners as triples of facelet indices, by their place: bit 0
// set for the right half, bit 1 for the top, bit 2 for the front.
function corners(n) {
  const model = new CubeModel(n);
  const out = Array.from({ length: 8 }, () => []);
  for (let i = 0; i < model.count; i++) {
    const { p } = model.place(i);
    if (p.filter((v) => Math.abs(v) >= n - 1).length === 3) {
      out[(p[0] > 0 ? 1 : 0) | (p[1] > 0 ? 2 : 0) | (p[2] > 0 ? 4 : 0)].push(i);
    }
  }
  return out;
}

// Which face each group of an even cube belongs on, from its corners: two
// colours that never meet at a corner are opposite, and the corner at the
// bottom, back and left is taken to be at home, which fixes the rest.
function nameByCorner(n, labels) {
  const meet = Array.from({ length: 6 }, () => new Array(6).fill(false));
  const cs = corners(n);
  for (const c of cs) {
    const [x, y, z] = c.map((i) => labels[i]);
    if (x === y || y === z || x === z) return null;
    meet[x][y] = meet[y][x] = meet[x][z] = meet[z][x] = meet[y][z] = meet[z][y] = true;
  }
  const opposite = [];
  for (let a = 0; a < 6; a++) {
    const never = [0, 1, 2, 3, 4, 5].filter((b) => b !== a && !meet[a][b]);
    if (never.length !== 1) return null;
    opposite[a] = never[0];
  }
  const model = new CubeModel(n);
  const name = new Array(6).fill(-1);
  for (const i of cs[0]) {
    const face = model.place(i).face;
    name[labels[i]] = face;
    name[opposite[labels[i]]] = (face + 3) % 6;
  }
  return name.includes(-1) || new Set(name).size !== 6 ? null : name;
}

/**
 * Sorts the camera's samples into the six colours of a cube.
 *
 * @param {number[][]} samples 6*n*n colours as [r, g, b], in facelet order
 * @param {number} n the size of the cube
 * @returns facelets (0 to 5 for URFDLB, per sample), `doubt` (0 to 1 per
 *   sample: how nearly it belonged to another colour), `palette` (the page
 *   colour to show each face's stickers in) and `seen` (the mean colour the
 *   camera saw for each face's stickers).
 */
export function classify(samples, n) {
  const nn = n * n;
  if (samples.length !== 6 * nn) throw new Error("expected 6 faces of n by n samples");
  const lab = samples.map(feature);
  let best;
  if (n % 2 === 1) {
    const mid = Math.floor(nn / 2);
    best = group(lab, [0, 1, 2, 3, 4, 5].map((f) => f * nn + mid), true);
  } else {
    // Six samples far apart: one to begin with, then each time the one
    // furthest from all chosen so far. A glint or a shadow can be one of
    // them and spoil the start, so several beginnings are tried and the
    // grouping whose groups are tightest is kept.
    const colourful = (i) => Math.hypot(lab[i][1], lab[i][2]);
    const byColour = lab.map((_, i) => i).sort((i, j) => colourful(j) - colourful(i));
    const starts = [0, 0.2, 0.4, 0.6, 0.8, 1].map((q) => byColour[Math.round(q * (byColour.length - 1))]);
    for (const first of starts) {
      const seeds = [first];
      const near = lab.map((p) => distance(p, lab[first]));
      while (seeds.length < 6) {
        let far = 0;
        near.forEach((d, i) => {
          if (d > near[far]) far = i;
        });
        seeds.push(far);
        lab.forEach((p, i) => (near[i] = Math.min(near[i], distance(p, lab[far]))));
      }
      const tried = group(lab, seeds, false);
      if (!best || tried.spread < best.spread) best = tried;
    }
  }
  const { labels, doubt } = best;

  const mean = (k) => {
    const sum = [0, 0, 0];
    for (let i = 0; i < labels.length; i++) if (labels[i] === k) for (let d = 0; d < 3; d++) sum[d] += samples[i][d];
    return sum.map((v) => v / nn);
  };
  const groupColour = [0, 1, 2, 3, 4, 5].map(mean);
  // The page colour nearest each group, each used once.
  const reference = STICKERS.map((hex) => feature(hexToRgb(hex)));
  const look = bestMatch(groupColour.map((c) => reference.map((r) => distance(feature(c), r))));

  // Odd: group k grew from the middle of face k. Even: ask a corner, and
  // if the corners make no sense yet, go by what the colours look like.
  const name = n % 2 === 1 ? [0, 1, 2, 3, 4, 5] : nameByCorner(n, labels) || look;
  const facelets = Uint8Array.from(labels, (k) => name[k]);
  const palette = new Array(6);
  const seen = new Array(6);
  for (let k = 0; k < 6; k++) {
    palette[name[k]] = STICKERS[look[k]];
    seen[name[k]] = groupColour[k].map(Math.round);
  }
  return { facelets, doubt, palette, seen };
}

/**
 * The colour of one cell of the camera's picture: the mean of its pixels
 * without the darkest and the lightest fifth, so that a glint of light or
 * the black between stickers does not pull it.
 *
 * @param {Uint8ClampedArray} data RGBA pixels of the cell
 */
export function cellColour(data) {
  const px = [];
  for (let i = 0; i < data.length; i += 4) px.push([data[i], data[i + 1], data[i + 2]]);
  px.sort((p, q) => p[0] + p[1] + p[2] - (q[0] + q[1] + q[2]));
  const cut = Math.floor(px.length / 5);
  const kept = px.slice(cut, px.length - cut);
  const sum = [0, 0, 0];
  for (const p of kept) for (let d = 0; d < 3; d++) sum[d] += p[d];
  return sum.map((v) => v / Math.max(1, kept.length));
}
