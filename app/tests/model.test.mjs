// The page's cube model against the solver's, and the notation people type.
//
//   node --test app/tests/*.test.mjs
//
// The tests that compare with the solver need its WebAssembly build:
//   cargo build --release -p anycube-wasm --target wasm32-unknown-unknown
// Without it they are skipped, and say so.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { CubeModel, ScrambleError, scrambleLength } from "../js/cube-model.js";
import { WasmSolver } from "../js/wasm-solver.js";

const built = new URL("../../target/wasm32-unknown-unknown/release/anycube_wasm.wasm", import.meta.url);
const wasm = existsSync(built) ? await WasmSolver.create(readFileSync(built)) : null;
const needsWasm = { skip: wasm ? false : "the WebAssembly build is not there (see the top of this file)" };

// The same numbers every run.
function random(seed) {
  let s = BigInt(seed);
  return () => {
    s = (s * 6364136223846793005n + 1442695040888963407n) & ((1n << 64n) - 1n);
    return Number(s >> 33n) / 2 ** 31;
  };
}

test("moves change the stickers as in the solver's model, sizes 2 to 9", needsWasm, () => {
  for (let n = 2; n <= 9; n++) {
    const model = new CubeModel(n);
    const rnd = random(n);
    for (let round = 0; round < 6; round++) {
      const moves = model.randomScramble(30 + 10 * n, rnd);
      const here = model.toText(model.applyAll(model.solved(), moves));
      const there = wasm.check(n, "moves", model.names(moves));
      assert.equal(there.ok, true);
      assert.equal(here, there.facelets, `size ${n}`);
    }
  }
});

test("names are read back as written", () => {
  const model = new CubeModel(7);
  for (let m = 0; m < model.moveCount; m++) {
    assert.deepEqual(model.parse(model.moveName(m)), [m]);
    assert.deepEqual(model.normalise(model.moveName(m)).moves, [m]);
  }
  assert.equal(model.moveName(model.move(1, 3, 2)), "3Rw'");
  assert.throws(() => model.parse("4Rw"), ScrambleError);
  assert.throws(() => model.parse("x"), ScrambleError);
  assert.throws(() => model.parse("3R"), ScrambleError);
});

test("an inverse undoes a move", () => {
  const model = new CubeModel(5);
  const start = model.applyAll(model.solved(), model.randomScramble(40, random(9)));
  for (let m = 0; m < model.moveCount; m++) {
    assert.deepEqual(model.apply(model.apply(start, m), model.inverse(m)), start);
  }
});

const state = (n, text) => {
  const model = new CubeModel(n);
  return model.toText(model.applyAll(model.solved(), model.normalise(text).moves));
};

test("what people type: lower case, apostrophes, brackets", () => {
  assert.equal(new CubeModel(4).normalise("r u2 f'").text, "Rw Uw2 Fw'");
  assert.equal(new CubeModel(3).normalise("(R U R’ U’), F2;").text, "R U R' U' F2");
  assert.equal(new CubeModel(3).normalise("R2' U2").text, "R2 U2");
  assert.equal(new CubeModel(5).normalise("  ").text, "");
  for (const bad of ["Q", "R3", "2R", "Rw'2", "RU", "xw", "3x", "0Rw"]) {
    assert.throws(() => new CubeModel(5).normalise(bad), ScrambleError, bad);
  }
  assert.throws(() => new CubeModel(4).normalise("M"), /middle slice/);
  assert.throws(() => new CubeModel(4).normalise("5Rw"), /more layers/);
});

test("whole-cube turns are folded into the moves after them", () => {
  // Turned like U, the back is on the right; turned like R, the front is on top.
  assert.equal(new CubeModel(3).normalise("y R U R' U' y'").text, "B U B' U'");
  assert.equal(new CubeModel(3).normalise("x U").text, "F");
  assert.equal(new CubeModel(3).normalise("z U").text, "L");
  assert.equal(new CubeModel(3).normalise("x x' y2 y2 z z z z R").text, "R");
  // Turning the whole cube and back changes nothing.
  assert.equal(state(5, "x y z Rw U2 z' y' x'"), state(5, "x y z Rw U2 z' y' x'"));
  assert.equal(state(4, "y' x R Uw x' y"), state(4, new CubeModel(4).normalise("y' x R Uw x' y").text));
});

test("middle slices and turns wider than half the cube", () => {
  // The checkerboard, both ways of making it.
  assert.equal(state(3, "M2 E2 S2"), state(3, "U2 D2 F2 B2 L2 R2"));
  assert.equal(state(5, "M2 E2 S2"), state(5, "2Uw2 2Dw2 2Fw2 2Bw2 2Lw2 2Rw2"));
  // A slice turned four times is back where it was, and so is the cube.
  assert.equal(state(3, "M M M M"), state(3, ""));
  assert.equal(state(7, "E S E' S' S E S' E'"), state(7, ""));
  // On a 3x3x3 a wide R is an L seen from the other side.
  assert.equal(new CubeModel(3).normalise("r").text, "L");
  assert.equal(new CubeModel(3).normalise("r U").text, "L F");
  assert.equal(state(3, "r U r'"), state(3, "L F L'"));
  // All the layers at once is the whole cube.
  assert.equal(new CubeModel(3).normalise("3Rw U").text, "F");
  // Four of five layers from the right is one from the left.
  assert.equal(new CubeModel(5).normalise("4Rw").text, "L");
  // The well-known edge flip written with slices: two edges turned over, nothing else.
  const flipped = state(3, "M' U M' U M' U2 M U M U M U2");
  assert.equal([...flipped].filter((c, i) => c !== "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"[i]).length, 4);
});

test("the browser's solver solves, and its answers hold on the page's model", needsWasm, () => {
  for (const n of [2, 3]) {
    const model = new CubeModel(n);
    const rnd = random(100 + n);
    for (let round = 0; round < 8; round++) {
      const scramble = model.randomScramble(scrambleLength(n), rnd);
      const start = model.applyAll(model.solved(), scramble);
      for (const [kind, input] of [["moves", model.names(scramble)], ["facelets", model.toText(start)]]) {
        const r = wasm.solve(n, kind, "fast", input);
        assert.equal(r.ok, true, r.message);
        assert.ok(model.isSolved(model.applyAll(start, model.parse(r.solution))), `size ${n} from ${kind}`);
        assert.ok(r.solution.split(" ").length <= (n === 2 ? 11 : 24));
      }
    }
  }
});

test("the browser says no to cubes that cannot be", needsWasm, () => {
  const model = new CubeModel(3);
  const f = model.solved();
  // One corner turned in its place.
  [f[8], f[9], f[20]] = [f[9], f[20], f[8]];
  const r = wasm.solve(3, "facelets", "fast", model.toText(f));
  assert.deepEqual([r.ok, r.code, r.impossible], [false, "twisted_corner", true]);
  assert.equal(wasm.solve(4, "moves", "fast", "Rw U").code, "not_in_browser");
});

test("a layer turned by hand is the moves that do the same", () => {
  for (const n of [3, 4, 5, 6]) {
    const model = new CubeModel(n);
    const start = model.applyAll(model.solved(), model.randomScramble(40, random(n)));
    for (let face = 0; face < 6; face++) {
      for (let layer = 0; layer < n; layer++) {
        for (const quarters of [1, 2, -1]) {
          const power = quarters === 1 ? 0 : quarters === 2 ? 1 : 2;
          const direct = model.permute(start, model.slab(face, layer, layer + 1, power));
          const { moves, rotate } = model.handTurn(face, layer, quarters);
          let byMoves = model.applyAll(start, moves);
          // The middle layer of an odd cube: the same cube, turned as a whole by as much.
          assert.equal(rotate, n % 2 === 1 && layer === (n - 1) / 2);
          if (rotate) byMoves = model.permute(byMoves, model.slab(face, 0, n, power));
          assert.equal(model.toText(byMoves), model.toText(direct), `size ${n} face ${face} layer ${layer} by ${quarters}`);
          assert.ok(moves.length <= 2);
        }
      }
    }
  }
  // An odd cube keeps its centres at home whatever is turned, so it can always be sent to a solver.
  const model = new CubeModel(5);
  let f = model.solved();
  for (const [face, layer, q] of [[1, 2, 1], [0, 2, -1], [2, 2, 2], [4, 1, 1]]) f = model.applyAll(f, model.handTurn(face, layer, q).moves);
  for (let face = 0; face < 6; face++) assert.equal(f[face * 25 + 12], face);
});

test("moves of the same layers that follow each other become one, or none", () => {
  const m = new CubeModel(4);
  const simple = (text) => m.names(m.simplify(m.parse(text)));
  assert.equal(simple("R R"), "R2");
  assert.equal(simple("R R'"), "");
  assert.equal(simple("R2 R2 U"), "U");
  assert.equal(simple("R R2"), "R'");
  assert.equal(simple("U R R' U'"), "");
  assert.equal(simple("R Rw R'"), "R Rw R'");
  assert.equal(simple("R U R' U'"), "R U R' U'");
});

test("random-move scrambles: the lengths competitions use, and no move undoing the one before", () => {
  assert.deepEqual([4, 5, 6, 7, 10].map(scrambleLength), [40, 60, 80, 100, 160]);
  for (const n of [4, 7, 12]) {
    const model = new CubeModel(n);
    const moves = model.randomScramble(scrambleLength(n), random(n * 3));
    assert.equal(moves.length, scrambleLength(n));
    assert.deepEqual(model.simplify(moves), moves, "nothing merges");
    for (let k = 2; k < moves.length; k++) {
      const [a, b, c] = [moves[k - 2], moves[k - 1], moves[k]].map((x) => model.info(x).face);
      assert.ok(!(a === c && a % 3 === b % 3), "no face, its opposite, and the face again");
    }
  }
});

test("random states from the browser's solver are cubes, each time another", needsWasm, () => {
  for (const n of [2, 3]) {
    const model = new CubeModel(n);
    const seen = new Set();
    for (let k = 0; k < 12; k++) {
      const text = wasm.randomState(n, [k * 7919 + 1, k]);
      const moves = model.parse(text);
      assert.ok(moves.length >= 4 && moves.length <= (n === 2 ? 11 : 24), text);
      seen.add(model.toText(model.applyAll(model.solved(), moves)));
    }
    assert.equal(seen.size, 12);
  }
  assert.equal(wasm.randomState(4, [1, 2]), "");
});

test("the middles of an odd cube's faces are set, however the stickers came", () => {
  const blank = 255;
  for (const n of [3, 5, 7]) {
    const model = new CubeModel(n);
    const nn = n * n;
    // Nothing known yet, as when a scan is abandoned or a cube is cleared.
    const empty = model.withCentres(new Uint8Array(model.count).fill(blank));
    for (let face = 0; face < 6; face++) {
      for (let k = 0; k < nn; k++) assert.equal(empty[face * nn + k], k === (nn - 1) / 2 ? face : blank);
    }
    // A cube that has them already comes back as it is, the very same array.
    const solved = model.solved();
    assert.equal(model.withCentres(solved), solved);
    // A wrong one is put right without touching the array given.
    const wrong = model.solved();
    wrong[(nn - 1) / 2] = 3;
    const fixed = model.withCentres(wrong);
    assert.equal(fixed[(nn - 1) / 2], 0);
    assert.equal(wrong[(nn - 1) / 2], 3);
  }
  // An even cube has no middles: nothing is set.
  const even = new CubeModel(4);
  const empty = new Uint8Array(even.count).fill(blank);
  assert.equal(even.withCentres(empty), empty);
});

test("a solution survives turns it did not ask for", () => {
  let seed = 7;
  const random = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
  for (const n of [3, 4, 5]) {
    const model = new CubeModel(n);
    const scramble = model.randomScramble(30, random);
    // A solution of the scrambled cube: the scramble backwards.
    const solution = scramble.map((m) => model.inverse(m)).reverse();
    const start = model.applyAll(model.solved(), scramble);
    for (let at = 0; at < solution.length; at += 3) {
      const here = model.applyAll(start, solution.slice(0, at));
      const rest = solution.slice(at);
      // The move the solution asks for next, made by hand: the solution steps on.
      const next = model.rejoin(rest, [rest[0]]);
      assert.deepEqual(next.moves, rest.slice(1));
      assert.equal(next.kept, next.moves.length);
      // Any layer turned by hand: what is left still solves the cube as it now is.
      for (let layer = 0; layer < n; layer++) {
        for (const quarters of [1, 2, -1]) {
          const face = Math.floor(random() * 6);
          const { moves } = model.handTurn(face, layer, quarters);
          const after = model.applyAll(here, moves);
          const left = model.rejoin(rest, moves);
          assert.ok(model.isSolved(model.applyAll(after, left.moves)), `${n}x${n}x${n}, after ${at} moves, layer ${layer} of face ${face}`);
          assert.ok(left.moves.length <= rest.length + moves.length);
          assert.deepEqual(left.moves.slice(left.moves.length - left.kept), rest.slice(rest.length - left.kept));
        }
      }
    }
  }
});

test("inner layers alone are a move: the same as the solver's moves that make them", () => {
  for (const n of [4, 5, 6, 7]) {
    const model = new CubeModel(n);
    const rnd = random(40 + n);
    const start = model.applyAll(model.solved(), model.randomScramble(30, rnd));
    for (let face = 0; face < 6; face++) {
      for (let from = 0; from < Math.floor(n / 2); from++) {
        for (let to = from + 1; to <= Math.floor(n / 2); to++) {
          for (let power = 0; power < 3; power++) {
            const m = model.slice(face, from, to, power);
            const info = model.info(m);
            assert.deepEqual([info.face, info.from, info.to, info.power], [face, from, to, power]);
            assert.deepEqual(model.apply(start, m), model.applyAll(start, model.plain([m])), `${n}: ${model.moveName(m)}`);
            assert.deepEqual(model.apply(model.apply(start, m), model.inverse(m)), start);
          }
        }
      }
    }
    assert.equal(model.moveName(model.slice(2, 1, 2, 0)), "2F");
    if (n >= 6) assert.equal(model.moveName(model.slice(2, 1, 3, 2)), "2-3Fw'");
    // A layer turned by hand is that one move, from whichever face it is nearer to.
    for (let face = 0; face < 6; face++) {
      for (let layer = 0; layer < n; layer++) {
        for (const quarters of [1, 2, -1]) {
          const one = model.handSlice(face, layer, quarters);
          if (n % 2 === 1 && layer === (n - 1) / 2) {
            assert.equal(one, null);
            continue;
          }
          assert.deepEqual(model.apply(start, one), model.applyAll(start, model.handTurn(face, layer, quarters).moves), `${n}: face ${face} layer ${layer} by ${quarters}`);
        }
      }
    }
  }
});

test("one layer of a wide move made by hand leaves the move's other layers, nothing undone", () => {
  for (const n of [4, 5, 6, 7]) {
    const model = new CubeModel(n);
    const rnd = random(60 + n);
    const start = model.applyAll(model.solved(), model.randomScramble(30, rnd));
    for (let face = 0; face < 6; face++) {
      for (let layers = 2; layers <= Math.floor(n / 2); layers++) {
        for (let power = 0; power < 3; power++) {
          const wide = model.move(face, layers, power);
          const want = model.apply(start, wide);
          for (let layer = 0; layer < layers; layer++) {
            for (const quarters of [1, 2, -1]) {
              const made = model.handSlice(face, layer, quarters);
              const left = model.remainder(wide, made);
              const name = `${n}: ${model.moveName(wide)} after ${model.moveName(made)}`;
              assert.deepEqual(model.applyAll(model.apply(start, made), left), want, name);
              // The layer turned as the move says is done with: only the others are left, as the move had them.
              if (quarters === model.info(wide).quarters) {
                assert.ok(left.every((m) => model.info(m).power === power && (model.info(m).to <= layer || model.info(m).from > layer)), name);
                assert.ok(left.length <= 2, name);
              }
              // And each of those in its turn steps the list on.
              let rest = left;
              for (const m of left) {
                assert.equal(model.rejoin(rest, [m]).moves.length, rest.length - 1, name);
                rest = rest.slice(1);
              }
            }
          }
        }
      }
    }
    // A turn that is not of the move's layers, or is all of them, leaves nothing of this kind.
    assert.equal(model.remainder(model.move(0, 2, 0), model.move(1, 1, 0)), null);
    assert.equal(model.remainder(model.move(0, 2, 0), model.move(0, 2, 2)), null);
    assert.equal(model.remainder(model.move(0, 1, 1), model.move(0, 1, 0)), null);
  }
});
