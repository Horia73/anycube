// Tests of finding a face in a picture, on the made-up pictures of
// pictures.mjs, where the truth is known.
//
//   node --test app/tests/*.test.mjs

import assert from "node:assert/strict";
import { test } from "node:test";
import { FACE_SCORE, findFace, invertHomography, solveAffine, solveHomography, warpPoint } from "../js/detect.js";
import { BEYOND, CASES, HARDER, UNIT, build, clutterOnly, cornerError, even, picture, quad, randomPose, wrongCells } from "./pictures.mjs";

// Runs the cases for one size and says what came of them.
function sweep(t, n, cases, seeds, W = 320, H = 240) {
  let worst = 0;
  let ms = 0;
  let calls = 0;
  for (const c of cases) {
    for (let seed = 1; seed <= seeds; seed++) {
      const { image, corners, stickers, light } = build(n, c, seed, W, H);
      const t0 = performance.now();
      const face = findFace(image, n);
      ms += performance.now() - t0;
      calls++;
      assert.ok(face, `${c.name}, picture ${seed}: no face found`);
      assert.ok(face.score >= FACE_SCORE);
      const error = cornerError(face.corners, corners);
      worst = Math.max(worst, error);
      assert.ok(error <= 0.04, `${c.name}, picture ${seed}: corners off by ${(error * 100).toFixed(1)}% of the side`);
      assert.equal(face.cells.length, n * n);
      assert.equal(wrongCells(face.cells, n, corners, stickers, light), 0, `${c.name}, picture ${seed}: colours`);
    }
  }
  t.diagnostic(`${n}x${n}x${n}: ${calls} pictures of ${W} by ${H}, corners within ${(worst * 100).toFixed(1)}% of the side, ${(ms / calls).toFixed(1)} ms a picture`);
}

test("homographies: solved, applied and undone", () => {
  const to = [
    [40, 30],
    [200, 44],
    [214, 190],
    [30, 170],
  ];
  const h = solveHomography(UNIT, to);
  UNIT.forEach(([x, y], k) => {
    const [X, Y] = warpPoint(h, x, y);
    assert.ok(Math.hypot(X - to[k][0], Y - to[k][1]) < 1e-6);
  });
  const back = invertHomography(h);
  to.forEach(([X, Y], k) => {
    const [x, y] = warpPoint(back, X, Y);
    assert.ok(Math.hypot(x - UNIT[k][0], y - UNIT[k][1]) < 1e-9);
  });
  // More points than needed: the same map from five of its own points.
  const five = [...UNIT, [0.3, 0.7]];
  const again = solveHomography(
    five,
    five.map(([x, y]) => warpPoint(h, x, y)),
  );
  again.forEach((v, k) => assert.ok(Math.abs(v - h[k]) < 1e-6 * Math.max(1, Math.abs(h[k]))));
  // Without perspective.
  const a = solveAffine(UNIT.slice(0, 3), to.slice(0, 3));
  assert.deepEqual(a.slice(6), [0, 0, 1]);
  UNIT.slice(0, 3).forEach(([x, y], k) => {
    const [X, Y] = warpPoint(a, x, y);
    assert.ok(Math.hypot(X - to[k][0], Y - to[k][1]) < 1e-9);
  });
  // Too few points, and points in a line, settle nothing.
  assert.equal(solveHomography(UNIT.slice(0, 3), to.slice(0, 3)), null);
  const line = [
    [0, 0],
    [1, 1],
    [2, 2],
  ];
  assert.equal(solveAffine(line, line), null);
});

for (const n of [3, 4, 5]) {
  test(`a ${n}x${n}x${n} face is found wherever and however it is held`, (t) => sweep(t, n, CASES, 2));
}

test("a 2x2x2 face, with four stickers to go by", (t) => sweep(t, 2, CASES, 2));

test("a small light-framed 2x2 with a logo still reads the dark sticker", () => {
  const corners = quad(163, 147, 64);
  const colours = [[60, 55, 50], [115, 42, 36], [55, 110, 61], [175, 151, 99], [155, 87, 65], [50, 70, 110]];
  const stickers = [0, 0, 4, 3];
  const image = picture({ W: 288, H: 288, n: 2, corners, stickers, colours, body: [167, 154, 140], fill: 0.85, noise: 2 });
  // A broad, light logo in the centre of the top-left sticker.
  for (let y = 120; y <= 141; y++) {
    for (let x = 137; x <= 155; x++) {
      if (x < 139 + Math.abs(y - 131) || x > 145 + Math.abs(y - 131)) continue;
      image.data.set([160, 148, 135], (y * image.width + x) * 4);
    }
  }
  const face = findFace(image, 2);
  assert.ok(face);
  assert.ok(cornerError(face.corners, corners) < 0.04);
  face.cells.forEach((rgb, i) => {
    const difference = rgb.map((channel, d) => channel - colours[stickers[i]][d]);
    assert.ok(Math.hypot(...difference) < 12, `sticker ${i}: ${rgb}`);
  });
});

test("a 2x2 still has four stickers when yellow is almost as bright as the frame", () => {
  const corners = quad(175, 123, 70);
  const colours = [[60, 55, 50], [178, 154, 101]];
  const stickers = [0, 1, 1, 1];
  const image = picture({ W: 288, H: 288, n: 2, corners, stickers, colours, body: [167, 154, 140], fill: 0.81, noise: 1 });
  const face = findFace(image, 2);
  assert.ok(face);
  assert.ok(cornerError(face.corners, corners) < 0.04);
  assert.ok(face.cells[0][0] < 80);
  assert.ok(face.cells.slice(1).every((rgb) => rgb[0] > 160));
});

for (const n of [6, 7]) {
  test(`a ${n}x${n}x${n} face, in a picture of 480 by 360`, (t) =>
    sweep(
      t,
      n,
      CASES.filter((c) => c.side >= 0.4),
      1,
      480,
      360,
    ));
}

test("stickers of one colour side by side, and a face of one colour", () => {
  for (const n of [3, 4]) {
    // A row of one colour, a block of one colour, and all of the face in one colour.
    const row = Array.from({ length: n * n }, (_, i) => (i < n ? 1 : (i * 7) % 6));
    const block = Array.from({ length: n * n }, (_, i) => (i % n < 2 && i < 2 * n ? 5 : (i * 5 + 2) % 6));
    const whole = new Array(n * n).fill(3);
    for (const stickers of [row, block, whole]) {
      const corners = quad(168, 116, 120, 8);
      const image = picture({ n, corners, stickers, seed: 5 });
      const face = findFace(image, n);
      assert.ok(face, `${n}x${n}x${n}: no face found`);
      assert.ok(cornerError(face.corners, corners) <= 0.04);
      assert.equal(wrongCells(face.cells, n, corners, stickers, even), 0);
    }
  }
});

test("no cube, no face", () => {
  for (const n of [2, 3, 4, 5]) {
    assert.equal(findFace(picture({ n, corners: null, stickers: [] }), n), null, "a plain background");
    assert.equal(findFace(picture({ n, corners: null, stickers: [], light: (x, y) => 0.5 + x / 640 + y / 960 }), n), null, "a background in uneven light");
    for (let seed = 1; seed <= 10; seed++) {
      assert.equal(findFace(clutterOnly(n, seed), n), null, `clutter ${seed}, looking for a ${n}x${n}x${n}`);
    }
  }
});

test("a face held any way within what is promised: how often it is found", (t) => {
  // Anywhere in the picture, from a quarter to nine tenths of its height,
  // turned by up to 25 degrees, leaning a little, in even or uneven light,
  // with more or less noise, half of the time on clutter.
  for (const n of [2, 3, 4, 5]) {
    const count = 40;
    let found = 0;
    let right = 0;
    for (let seed = 1; seed <= count; seed++) {
      const pose = randomPose(n, seed);
      const face = findFace(pose.image, n);
      if (!face) continue;
      found++;
      if (pose.error(face) <= 0.04 && pose.wrong(face) === 0) right++;
    }
    t.diagnostic(`${n}x${n}x${n}: found in ${found} of ${count} pictures, corners and colours right in ${right}`);
    assert.ok(right >= count * 0.97, `${n}x${n}x${n}: right in ${right} of ${count}`);
    // A face found in the wrong place is worse than none: next to never.
    assert.ok(found - right <= 1, `${n}x${n}x${n}: ${found - right} found in the wrong place`);
  }
});

test("a cube as it is held, and cubes of other makes", (t) => {
  for (const [name, make] of Object.entries(HARDER)) {
    for (const n of [2, 3, 4]) {
      let right = 0;
      for (let seed = 1; seed <= 10; seed++) {
        const { image, corners, stickers, light } = make(n, seed);
        const face = findFace(image, n);
        if (!face) continue;
        assert.ok(cornerError(face.corners, corners) <= 0.04, `${name}, ${n}x${n}x${n}, picture ${seed}: the wrong place`);
        assert.equal(wrongCells(face.cells, n, corners, stickers, light), 0, `${name}, ${n}x${n}x${n}, picture ${seed}: colours`);
        right++;
      }
      assert.ok(right >= 9, `${name}, ${n}x${n}x${n}: found in ${right} of 10`);
    }
  }
  // Beyond what is promised a face may not be found; one found is still the right one.
  for (const [name, make] of Object.entries(BEYOND)) {
    let found = 0;
    for (let seed = 1; seed <= 10; seed++) {
      const { image, corners, stickers, light } = make(3, seed);
      const face = findFace(image, 3);
      if (!face) continue;
      found++;
      assert.ok(cornerError(face.corners, corners) <= 0.04, `${name}, picture ${seed}: the wrong place`);
      assert.equal(wrongCells(face.cells, 3, corners, stickers, light), 0, `${name}, picture ${seed}: colours`);
    }
    t.diagnostic(`${name}: found in ${found} of 10`);
  }
});

test("the size asked for is the size found", () => {
  // A 3x3x3 in the picture and a 5x5x5 asked for: nothing, rather than a lattice forced on it.
  const corners = quad(160, 120, 130, 6);
  const image = picture({ n: 3, corners, stickers: [0, 1, 2, 3, 4, 5, 1, 2, 3], seed: 9 });
  assert.ok(findFace(image, 3));
  assert.equal(findFace(image, 5), null);
});
