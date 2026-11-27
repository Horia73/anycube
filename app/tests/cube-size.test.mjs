import assert from "node:assert/strict";
import { test } from "node:test";
import { detectCubeSize, findFace } from "../js/detect.js";
import { clutterOnly, cornerError, even, picture, quad, wrongCells } from "./pictures.mjs";

function scene(n, cx, cy, W, H) {
  const corners = quad(cx, cy, 98, 7, 0.08, 0.05);
  const stickers = Array.from({ length: n * n }, (_, i) => (i * 5 + 1) % 6);
  return { image: picture({ W, H, n, corners, stickers }), corners, stickers };
}

for (const n of [2, 3, 4, 5, 6, 7]) {
  test(`infers the entire ${n} by ${n} grid outside a central square crop`, () => {
    for (const pose of [[65, 112, 480, 224], [115, 410, 224, 480]]) {
      const { image, corners, stickers } = scene(n, ...pose);
      const result = detectCubeSize(image);
      assert.equal(result.status, "detected");
      assert.equal(result.n, n);
      assert.ok(result.confidence >= 0.85 && result.confidence <= 1);
      assert.ok(cornerError(result.face.corners, corners) < 0.04);
      assert.equal(result.face.cells.length, n * n);
      assert.equal(wrongCells(result.face.cells, n, corners, stickers, even), 0);
    }
  });
}

test("a perfect three-by-three subgrid does not win inside four-by-four", () => {
  const corners = quad(160, 120, 144);
  const image = picture({ n: 4, corners, stickers: Array(16).fill(2) });
  assert.equal(findFace(image, 3).score, 1);
  assert.equal(detectCubeSize(image).n, 4);
});

test("incompatible complete grids are ambiguous", () => {
  const left = scene(3, 78, 112, 320, 224).image;
  const right = scene(4, 242, 112, 320, 224).image;
  for (let y = 0; y < left.height; y++) {
    const start = (y * left.width + 160) * 4;
    left.data.set(right.data.subarray(start, start + 640), start);
  }
  assert.deepEqual(detectCubeSize(left), { status: "ambiguous", n: null, confidence: 0, face: null });
});

test("blank, clutter, and a grid cut off along one side yield no size", () => {
  for (const image of [picture({ n: 3, corners: null, stickers: [] }), clutterOnly(4, 1), scene(4, 0, 120, 320, 240).image]) {
    assert.deepEqual(detectCubeSize(image), { status: "not-found", n: null, confidence: 0, face: null });
  }
});

test("a frame clipped on two seams cannot establish a smaller cube", () => {
  const image = picture({ n: 4, corners: quad(32, 32, 128), stickers: Array(16).fill(1) });
  assert.equal(detectCubeSize(image).status, "not-found");
});

test("an eight by eight grid exceeds the supported range", () => {
  assert.equal(detectCubeSize(scene(8, 160, 120, 320, 240).image).status, "not-found");
});
