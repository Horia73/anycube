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
// Red and orange are the pair that goes wrong, so the comparison is built
// for them:
//
// - A colour is a direction, not a point: the same sticker in shadow or
//   in a darker exposure is the same red, green and blue made smaller
//   together. A sample is compared with a group by how far it lies from
//   the line of that group's colour at every brightness, measured in the
//   camera's own noise (a few steps of its scale, whatever the
//   brightness), so a dark sample, which could be many colours, counts for
//   little, and a dark orange is not taken for a red because it is dark.
// - A different white balance multiplies the red, the green and the blue
//   of a whole picture, each by its own number. Those three numbers are
//   estimated for each of the six pictures, from how its samples sit
//   against the groups they fell into, and taken out; the camera changes
//   its white balance between one face and the next.
// - Every colour is on exactly N*N stickers, so all samples are placed
//   together, the surest first.
//
// No DOM here, so that the tests can load it under Node.

import { CubeModel } from "./cube-model.js";

// The page's sticker colours, in face order U R F D L B.
export const STICKERS = ["#f4f4ef", "#d92b3a", "#1faa59", "#f7d038", "#f58220", "#2f6fde"];

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

// A colour as three numbers that say what it looks like to the eye:
// lightness, with a small weight, and the two colour axes of CIELAB made
// nearly independent of the light. Used to find the page colour nearest a
// group of stickers, where the group's mean has little noise left.
export function feature(rgb) {
  const [L, a, b] = toLab(rgb);
  return [L * 0.25, (100 * a) / (L + 60), (100 * b) / (L + 60)];
}

// What the sorting itself compares: how far a sample lies from the line
// through black and a group's colour, in units of what the camera's noise
// and a sticker's own unevenness would move it.
const NOISE = 6; // the camera's noise, in steps of its 8-bit scale
const UNEVEN = 0.05; // how much one sticker's colour differs across it and from its fellows, as a share

function apart(sample, toward) {
  const [r, g, b] = sample;
  const along = r * toward[0] + g * toward[1] + b * toward[2];
  const length2 = r * r + g * g + b * b;
  const off2 = along > 0 ? Math.max(0, length2 - along * along) : length2;
  return Math.sqrt(off2 / (NOISE * NOISE + UNEVEN * UNEVEN * length2));
}

// Brightness says little: a shadow halves it. But a sample several times
// darker or lighter than a group is (a black sticker among coloured ones)
// is not of that group, whatever line its noise puts it nearest. Nothing is
// counted up to a difference a shadow could make, and more beyond it.
const SHADE = 0.9; // as a logarithm: about two and a half times
function unlike(length, usual) {
  const beyond = Math.abs(Math.log((length + NOISE) / (usual + NOISE))) - SHADE;
  return beyond > 0 ? 3 * beyond : 0;
}

function unit(v) {
  const length = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / length, v[1] / length, v[2] / length];
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
// `cast` is what to divide the samples of each picture by before comparing
// (see `sortColours`).
function group(samples, seeds, pinned, each, cast, start) {
  const total = samples.length;
  const at = samples.map((p, i) => {
    const c = cast[Math.floor(i / each)];
    return [p[0] / c[0], p[1] / c[1], p[2] / c[2]];
  });
  const length = at.map((p) => Math.hypot(p[0], p[1], p[2]));
  let centres = start?.centres ?? seeds.map((i) => unit(at[i]));
  // How bright each group's samples usually are.
  let usual = start?.usual ?? seeds.map((i) => length[i]);
  let labels = new Int8Array(total).fill(-1);
  const fixed = new Int8Array(total).fill(-1);
  if (pinned) seeds.forEach((i, k) => (fixed[i] = k));
  let cost = [];
  for (let round = 0; round < 16; round++) {
    cost = at.map((m, i) => centres.map((c, k) => Math.hypot(apart(m, c), unlike(length[i], usual[k]))));
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
    // The colour of each group: the directions of its samples laid end to
    // end, each counted for what it can say (a dark sample little). Not their
    // sum as they are: one bright sample that strayed into a group of dark
    // ones would then be that group's colour by itself.
    centres = centres.map((old, k) => {
      const sum = [0, 0, 0];
      for (let i = 0; i < total; i++) {
        if (labels[i] !== k) continue;
        const weight = 1 / Math.sqrt(at[i][0] ** 2 + at[i][1] ** 2 + at[i][2] ** 2 + 4 * NOISE * NOISE);
        for (let d = 0; d < 3; d++) sum[d] += at[i][d] * weight;
      }
      return sum[0] + sum[1] + sum[2] > 0 ? unit(sum) : old;
    });
    usual = usual.map((old, k) => {
      let sum = 0;
      let count = 0;
      for (let i = 0; i < total; i++) {
        if (labels[i] !== k) continue;
        sum += Math.log(length[i] + NOISE);
        count++;
      }
      return count ? Math.exp(sum / count) - NOISE : old;
    });
  }
  // How unsure each sample is: 0 when its group is far nearer than any other, 1 when two are
  // as near; and which group that other is.
  const doubt = new Float32Array(total);
  const second = new Int8Array(total);
  for (let i = 0; i < total; i++) {
    const own = cost[i][labels[i]];
    let other = -1;
    for (let c = 0; c < 6; c++) if (c !== labels[i] && (other < 0 || cost[i][c] < cost[i][other])) other = c;
    second[i] = other;
    doubt[i] = Math.max(0, Math.min(1, own / (cost[i][other] + 1e-6)));
  }
  let spread = 0;
  for (let i = 0; i < total; i++) spread += cost[i][labels[i]];
  return { labels, doubt, second, spread, centres, usual, at };
}

// What each picture's red, green and blue are multiplied by, next to the
// others: for each channel, the number that best turns the group colours
// of the picture's samples (each at its sample's brightness) into what the
// samples are. A face with one or two colours on it says little about that
// (the numbers and the colours cannot be told apart there), so its numbers
// are taken in part only, and left at one where a single colour fills the face.
function casts(sorted, each) {
  const { labels, centres, at } = sorted;
  const out = [];
  for (let f = 0; f < 6; f++) {
    const top = [0, 0, 0];
    const bottom = [0, 0, 0];
    const seen = new Set();
    for (let i = f * each; i < (f + 1) * each; i++) {
      const c = centres[labels[i]];
      seen.add(labels[i]);
      const along = Math.max(0, at[i][0] * c[0] + at[i][1] * c[1] + at[i][2] * c[2]);
      for (let d = 0; d < 3; d++) {
        top[d] += at[i][d] * along * c[d];
        bottom[d] += (along * c[d]) ** 2;
      }
    }
    const share = Math.max(0, Math.min(1, (seen.size - 1) / 3));
    const raw = top.map((t, d) => (bottom[d] > 1 ? Math.max(0.5, Math.min(2, t / bottom[d])) : 1));
    // Only how the three differ from each other: brightness is not a cast.
    const middle = Math.cbrt(raw[0] * raw[1] * raw[2]);
    out.push(raw.map((v) => (v / middle) ** share));
  }
  return out;
}

/**
 * Sorts the camera's samples into six groups of n*n, without naming them.
 * The samples are in six runs of n*n, one run per picture (in any order of
 * faces, each picture turned any way: the sorting does not depend on it).
 *
 * @returns `labels` (0 to 5 per sample; on a cube of odd size group k is
 *   the one the middle sample of run k is in), `doubt` (0 to 1 per sample:
 *   how nearly it belonged to another group), `second` (that other group)
 *   and `colours` (the mean colour the camera saw for each group).
 */
export function sortColours(samples, n) {
  const nn = n * n;
  if (samples.length !== 6 * nn) throw new Error("expected 6 faces of n by n samples");
  const none = Array.from({ length: 6 }, () => [1, 1, 1]);
  const run = (seeds, pinned) => {
    let cast = none;
    let best = group(samples, seeds, pinned, nn, cast, null);
    // The pictures' own casts, taken out a few times over: each sorting says them better.
    for (let round = 0; round < 3; round++) {
      const more = casts(best, nn);
      if (more.every((c) => c.every((v) => Math.abs(v - 1) < 0.004))) break;
      cast = cast.map((c, f) => c.map((v, d) => v * more[f][d]));
      best = group(samples, seeds, pinned, nn, cast, best);
    }
    return best;
  };
  let best;
  if (n % 2 === 1) {
    const mid = Math.floor(nn / 2);
    best = run([0, 1, 2, 3, 4, 5].map((f) => f * nn + mid), true);
  } else {
    // Six samples far apart: one to begin with, then each time the one
    // furthest from all chosen so far. A glint or a shadow can be one of
    // them and spoil the start, so several beginnings are tried and the
    // sorting whose groups are tightest is kept.
    const units = samples.map(unit);
    const between = (i, j) => Math.min(apart(samples[i], units[j]), apart(samples[j], units[i]));
    const grey = unit([1, 1, 1]);
    const colourful = (i) => apart(samples[i], grey);
    const byColour = samples.map((_, i) => i).sort((i, j) => colourful(j) - colourful(i));
    const starts = [0, 0.2, 0.4, 0.6, 0.8, 1].map((q) => byColour[Math.round(q * (byColour.length - 1))]);
    for (const first of starts) {
      const seeds = [first];
      const near = samples.map((_, i) => between(i, first));
      while (seeds.length < 6) {
        let far = 0;
        near.forEach((d, i) => {
          if (d > near[far]) far = i;
        });
        seeds.push(far);
        samples.forEach((_, i) => (near[i] = Math.min(near[i], between(i, far))));
      }
      const tried = run(seeds, false);
      if (!best || tried.spread < best.spread) best = tried;
    }
  }
  const { labels, doubt, second } = best;
  const colours = [0, 1, 2, 3, 4, 5].map((k) => {
    const sum = [0, 0, 0];
    for (let i = 0; i < labels.length; i++) if (labels[i] === k) for (let d = 0; d < 3; d++) sum[d] += samples[i][d];
    return sum.map((v) => v / nn);
  });
  return { labels, doubt, second, colours };
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
export function nameByCorner(n, labels) {
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
 *   sample: how nearly it belonged to another colour), `second` (that other
 *   colour, as a face), `palette` (the page colour to show each face's
 *   stickers in) and `seen` (the mean colour the camera saw for each face's
 *   stickers).
 */
export function classify(samples, n) {
  const sorted = sortColours(samples, n);
  return named(n, sorted, sorted.labels);
}

/**
 * Names the groups of `sortColours` by the faces they belong on, for
 * samples laid out as a cube: `labels` are the groups in facelet order
 * (the same groups, moved to where an arrangement of the pictures puts
 * them), `doubt` and `second` likewise if given.
 */
export function named(n, sorted, labels, doubt = sorted.doubt, second = sorted.second) {
  const nn = n * n;
  const groupColour = sorted.colours;
  // The page colour nearest each group, each used once.
  const reference = STICKERS.map((hex) => feature(hexToRgb(hex)));
  const look = bestMatch(groupColour.map((c) => reference.map((r) => distance(feature(c), r))));
  // Odd: a group is named by the face its middle sticker is on. Even: ask a
  // corner, and if the corners make no sense yet, go by what the colours look like.
  let name;
  if (n % 2 === 1) {
    const mid = Math.floor(nn / 2);
    name = new Array(6).fill(-1);
    for (let f = 0; f < 6; f++) name[labels[f * nn + mid]] = f;
    if (name.includes(-1)) name = look;
  } else {
    name = nameByCorner(n, labels) || look;
  }
  const facelets = Uint8Array.from(labels, (k) => name[k]);
  const palette = new Array(6);
  const seen = new Array(6);
  for (let k = 0; k < 6; k++) {
    palette[name[k]] = STICKERS[look[k]];
    seen[name[k]] = groupColour[k].map(Math.round);
  }
  return { facelets, doubt, second: Int8Array.from(second, (k) => name[k]), palette, seen };
}

// The page's own colours as directions, for a first guess at one sample.
const STANDARD = STICKERS.map((hex) => unit(hexToRgb(hex)));

/**
 * What one sample looks like by itself, before a whole cube has been seen:
 * the nearest of the page's six colours, and how nearly it was the next
 * nearest (0 to 1). Only for showing while the camera is held up: the cube
 * is sorted afterwards, all its stickers together, without this.
 */
export function guess(rgb) {
  const far = STANDARD.map((c, k) => Math.hypot(apart(rgb, c), k === 0 ? 0 : unlike(Math.hypot(rgb[0], rgb[1], rgb[2]), 255)));
  let first = 0;
  let next = -1;
  for (let k = 1; k < 6; k++) if (far[k] < far[first]) first = k;
  for (let k = 0; k < 6; k++) if (k !== first && (next < 0 || far[k] < far[next])) next = k;
  return { colour: first, doubt: Math.max(0, Math.min(1, far[first] / (far[next] + 1e-6))) };
}

/** How unlike two samples are, in the measure the sorting uses: under 3 or so they could be one colour. */
export function unalike(a, b) {
  return Math.max(apart(a, unit(b)), apart(b, unit(a)));
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
