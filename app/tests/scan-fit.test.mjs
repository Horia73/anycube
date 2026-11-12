// Six pictures put together into a cube: as taken, with a face held a
// quarter turn round, with faces shown in each other's turn, with every
// face anywhere and any way up, and with two doubtful stickers exchanged.
// `node --test app/tests/*.test.mjs`.

import assert from "node:assert/strict";
import { test } from "node:test";
import { CubeModel } from "../js/cube-model.js";
import { sortColours, named, oppositeCentres, STICKERS } from "../js/colour.js";
import { arrange, assemble, assembleScan, cubeKey, fit, reviewStickers, structure } from "../js/scan-fit.js";

function random(seed) {
  let s = BigInt(seed);
  return () => {
    s = (s * 6364136223846793005n + 1442695040888963407n) & ((1n << 64n) - 1n);
    return Number(s >> 33n) / 2 ** 31;
  };
}

const COLOURS = [[205, 208, 204], [176, 28, 38], [18, 138, 72], [212, 196, 44], [226, 104, 24], [22, 70, 168]];

// What a camera would give for a cube: each face under its own light, a little noise.
function photograph(facelets, n, rnd, noise = 4) {
  const nn = n * n;
  const gain = Array.from({ length: 6 }, () => 0.7 + rnd() * 0.4);
  return Array.from(facelets, (c, i) => COLOURS[c].map((v) => Math.max(0, Math.min(255, v * gain[Math.floor(i / nn)] + (rnd() + rnd() + rnd() - 1.5) * 2 * noise))));
}

// The pictures as someone took them who held faces round and mixed them up:
// `took[s]` is the face shown when face s was asked for, `held[s]` the
// quarter turns it was held round by.
function mistaken(n, samples, took, held) {
  return arrange(n, samples, { from: took, rot: held });
}

const scrambled = (n, rnd) => {
  const model = new CubeModel(n);
  return model.applyAll(model.solved(), model.randomScramble(20 * n, rnd));
};

// The same cube, however it was turned as a whole when it was put together.
const sameCube = (n, a, b) => cubeKey(n, a) === cubeKey(n, b);

const DISTINCT = {
  "white and pale yellow": {
    palette: [[203, 214, 167], [194, 25, 43], [16, 181, 94], [209, 192, 134], [240, 152, 75], [64, 87, 157]],
    exposure: [1, 0.96, 1.02, 1.01, 1, 1.06],
  },
  "warm white and muted blue": {
    palette: [[172, 166, 156], [154, 72, 64], [39, 131, 61], [191, 168, 63], [229, 88, 52], [73, 78, 96]],
    exposure: [0.94, 0.91, 0.91, 0.96, 1.1, 0.96],
  },
};

for (const [name, { palette, exposure }] of Object.entries(DISTINCT)) {
  test(`camera scans preserve distinct ${name}`, () => {
    for (const n of [3, 4]) {
      const model = new CubeModel(n);
      const truth = model.applyAll(model.solved(), model.parse("R U F2 D' B2 R2 L U' B L2 U2 F' D2 B'"));
      const samples = Array.from(truth, (colour, i) => palette[colour].map((channel) => Math.round(channel * exposure[Math.floor(i / (n * n))])));
      const { sorted, made } = assembleScan(n, samples);
      assert.equal(made.kind, "taken", `size ${n}`);
      const read = named(n, sorted, arrange(n, made.labels, made));
      assert.deepEqual(Array.from(read.facelets, (colour) => read.palette[colour]), Array.from(truth, (colour) => STICKERS[colour]));
    }
  });
}

test("scan review picks the most uncertain editable stickers", () => {
  const doubt = new Float32Array(54);
  doubt.set([0.78, 0.83, 0.41, 0.89, 1, 0.61, 0.91, 0.59]);
  assert.deepEqual(reviewStickers(3, doubt), [6, 3, 1, 0]);
  assert.deepEqual(reviewStickers(2, doubt.slice(0, 24)), [4, 6, 3, 1]);
  assert.deepEqual(reviewStickers(3, new Float32Array(54).fill(0.54)), []);
});

test("matte red and orange stay distinct on cubes with a black side", () => {
  const colours = [[60, 55, 50], [115, 42, 36], [55, 110, 61], [175, 151, 99], [136, 65, 40], [50, 70, 110]];
  const palette = ["#24252b", ...STICKERS.slice(1)];
  for (const n of [2, 3]) {
    const model = new CubeModel(n);
    const truth = model.applyAll(model.solved(), model.parse("R U F2 D' B2 R2 L U' B L2 U2 F' D2 B'"));
    const samples = Array.from(truth, (colour) => colours[colour]);
    const { sorted, made } = assembleScan(n, samples);
    assert.equal(made.kind, "taken");
    const read = named(n, sorted, arrange(n, made.labels, made));
    assert.ok(oppositeCentres(read.palette));
    assert.deepEqual(Array.from(read.facelets, (colour) => read.palette[colour]), Array.from(truth, (colour) => palette[colour]));
  }
});

test("what the outside of a cube says: real cubes pass, broken ones do not", () => {
  const rnd = random(1);
  for (let n = 2; n <= 7; n++) {
    const model = new CubeModel(n);
    assert.equal(structure(n, model.solved()), null);
    for (let k = 0; k < 20; k++) assert.equal(structure(n, scrambled(n, rnd)), null, `size ${n}`);
  }
  // A 3x3x3 with one corner twisted, one edge flipped, two edges exchanged, two stickers exchanged.
  const model = new CubeModel(3);
  const cube = scrambled(3, rnd);
  const corner = [0, 9 * 4 + 0, 9 * 5 + 2]; // top-left-back: U's first, L's first, B's third
  const twisted = cube.slice();
  [twisted[corner[0]], twisted[corner[1]], twisted[corner[2]]] = [cube[corner[1]], cube[corner[2]], cube[corner[0]]];
  assert.ok(["twist", "corner"].includes(structure(3, twisted)));
  const flipped = cube.slice();
  [flipped[7], flipped[9 * 2 + 1]] = [cube[9 * 2 + 1], cube[7]]; // top-front edge
  assert.equal(structure(3, flipped), "flip");
  const swapped = cube.slice();
  [swapped[7], swapped[5]] = [cube[5], cube[7]];
  [swapped[9 * 2 + 1], swapped[9 * 1 + 1]] = [cube[9 * 1 + 1], cube[9 * 2 + 1]]; // top-front with top-right
  assert.equal(structure(3, swapped), "parity");
  assert.equal(structure(3, model.solved()), null);
});

test("pictures taken as asked make their cube, and nothing is changed", () => {
  const rnd = random(2);
  for (const n of [2, 3, 4, 5]) {
    for (let k = 0; k < 6; k++) {
      const truth = scrambled(n, rnd);
      const sorted = sortColours(photograph(truth, n, rnd), n);
      const made = assemble(n, sorted);
      assert.equal(made.kind, "taken", `size ${n}`);
      assert.deepEqual(made.from, [0, 1, 2, 3, 4, 5]);
      assert.deepEqual(made.rot, [0, 0, 0, 0, 0, 0]);
      assert.ok(sameCube(n, named(n, sorted, arrange(n, made.labels, made)).facelets, truth));
    }
  }
});

test("a face held a quarter turn round, or upside down, is put right", () => {
  const rnd = random(3);
  for (const n of [3, 4, 5]) {
    for (let k = 0; k < 8; k++) {
      const truth = scrambled(n, rnd);
      const face = Math.floor(rnd() * 6);
      const held = [0, 0, 0, 0, 0, 0];
      held[face] = 1 + Math.floor(rnd() * 3);
      const samples = mistaken(n, photograph(truth, n, rnd), [0, 1, 2, 3, 4, 5], held);
      const sorted = sortColours(samples, n);
      const made = assemble(n, sorted);
      assert.equal(made.kind, "placed", `size ${n}, face ${face} held round by ${held[face]}`);
      assert.deepEqual(made.turned, [face]);
      assert.deepEqual(made.moved, []);
      assert.equal((made.rot[face] + held[face]) % 4, 0);
      assert.ok(sameCube(n, named(n, sorted, arrange(n, made.labels, made)).facelets, truth));
    }
  }
});

test("two faces shown in each other's turn are put in their places", () => {
  const rnd = random(4);
  for (const n of [3, 4]) {
    for (let k = 0; k < 8; k++) {
      const truth = scrambled(n, rnd);
      const a = Math.floor(rnd() * 6);
      const b = (a + 1 + Math.floor(rnd() * 5)) % 6;
      const took = [0, 1, 2, 3, 4, 5];
      [took[a], took[b]] = [took[b], took[a]];
      const sorted = sortColours(mistaken(n, photograph(truth, n, rnd), took, [0, 0, 0, 0, 0, 0]), n);
      const made = assemble(n, sorted);
      assert.equal(made.kind, "placed", `size ${n}, faces ${a} and ${b}`);
      assert.ok(sameCube(n, named(n, sorted, arrange(n, made.labels, made)).facelets, truth), `size ${n}, faces ${a} and ${b}`);
    }
  }
});

test("an odd cube scan with adjacent white and yellow centres cannot be accepted", () => {
  const n = 3;
  const truth = scrambled(n, random(42));
  // Simulate the camera assigning green and yellow to each other's group.
  // The pieces still form a cube if the wrong colour names are trusted.
  const colours = [0, 1, 3, 2, 4, 5];
  const samples = Array.from(truth, (c) => COLOURS[colours[c]]);
  const sorted = sortColours(samples, n);
  assert.equal(structure(n, named(n, sorted, sorted.labels).facelets), null);
  assert.equal(assemble(n, sorted).kind, "none");
});

test("six faces in any order and any way up make one cube, the right one", () => {
  const rnd = random(5);
  for (const n of [2, 3, 4, 5]) {
    let one = 0;
    const rounds = 10;
    for (let k = 0; k < rounds; k++) {
      const truth = scrambled(n, rnd);
      const took = [0, 1, 2, 3, 4, 5].sort(() => rnd() - 0.5);
      const held = took.map(() => Math.floor(rnd() * 4));
      const sorted = sortColours(mistaken(n, photograph(truth, n, rnd), took, held), n);
      const ways = fit(n, sorted);
      if (ways.length === 1) one++;
      assert.ok(ways.some((w) => sameCube(n, named(n, sorted, arrange(n, sorted.labels, w)).facelets, truth)), `size ${n}: the cube is among the ${ways.length} found`);
    }
    // A scrambled cube almost never fits together in a second way; a 2x2x2 has least to go by.
    assert.ok(one >= (n === 2 ? rounds - 3 : rounds - 1), `size ${n}: ${one} of ${rounds} had one way`);
  }
});

test("a red and an orange sticker exchanged by the sorting are exchanged back", () => {
  const rnd = random(6);
  for (const n of [3, 4]) {
    let mended = 0;
    const rounds = 10;
    for (let k = 0; k < rounds; k++) {
      const truth = scrambled(n, rnd);
      const samples = photograph(truth, n, rnd, 2);
      // One red and one orange sticker photographed nearly alike, each a little on the other's side.
      const nn = n * n;
      const notMiddle = (i) => n % 2 === 0 || i % nn !== (nn - 1) / 2;
      const red = truth.findIndex((c, i) => c === 1 && notMiddle(i));
      const orange = truth.findIndex((c, i) => c === 4 && notMiddle(i));
      const mix = (t) => COLOURS[1].map((v, d) => v * (1 - t) + COLOURS[4][d] * t);
      samples[red] = mix(0.56);
      samples[orange] = mix(0.44);
      const { sorted, made } = assembleScan(n, samples);
      if (made.kind === "taken") continue; // the sorting got them right after all
      assert.equal(made.kind, "recoloured", `size ${n} round ${k}`);
      assert.deepEqual([...made.swapped].sort((a, b) => a - b), [red, orange].sort((a, b) => a - b));
      assert.ok(sameCube(n, named(n, sorted, arrange(n, made.labels, made)).facelets, truth));
      mended++;
    }
    assert.ok(mended >= 5, `size ${n}: ${mended} cubes needed mending and were mended`);
  }
});

test("a solved cube, which fits together in many ways, is taken as it was shown", () => {
  const rnd = random(7);
  for (const n of [2, 3, 4]) {
    const model = new CubeModel(n);
    const sorted = sortColours(photograph(model.solved(), n, rnd), n);
    const made = assemble(n, sorted);
    assert.equal(made.kind, "taken");
  }
});

test("under a shadow across each face, a 3x3x3 comes out right or is mended, not wrong unseen", () => {
  const rnd = random(8);
  const n = 3;
  const model = new CubeModel(n);
  const tally = { right: 0, mended: 0, refused: 0, unseen: 0 };
  for (let k = 0; k < 40; k++) {
    const truth = model.applyAll(model.solved(), model.randomScramble(50, rnd));
    // Each face with its own light, and darker by half from some row or column on.
    const faces = Array.from({ length: 6 }, () => ({ gain: 0.6 + rnd() * 0.4, across: rnd() < 0.5, from: Math.floor(rnd() * n), dark: rnd() < 0.8 ? 0.45 : 1 }));
    const samples = Array.from(truth, (c, i) => {
      const f = faces[Math.floor(i / 9)];
      const shade = (f.across ? Math.floor((i % 9) / 3) : i % 3) >= f.from ? f.dark : 1;
      return COLOURS[c].map((v) => Math.max(0, Math.min(255, v * f.gain * shade + (rnd() + rnd() + rnd() - 1.5) * 10)));
    });
    const sorted = sortColours(samples, n);
    const made = assemble(n, sorted);
    if (made.kind === "none" || made.kind === "several") tally.refused++;
    else if (!sameCube(n, named(n, sorted, arrange(n, made.labels, made)).facelets, truth)) tally.unseen++;
    else if (made.kind === "taken") tally.right++;
    else tally.mended++;
  }
  assert.equal(tally.unseen, 0, JSON.stringify(tally));
  assert.ok(tally.right + tally.mended >= 38, JSON.stringify(tally));
});
