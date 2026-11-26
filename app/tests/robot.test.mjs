// The robot extension's core (js/robot.js): the square cut out of a frame
// and turned, the faces read, the time left, the server's API, and whole
// runs against a mock of the server whose events are scripted the way the
// robot behaves (robots-spec/API.md): it waits for each photo-taken.

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { STICKERS } from "../js/colour.js";
import { CubeModel } from "../js/cube-model.js";
import {
  assembleFaces, Eta, frameSize, GRID_SHARE, gridCells, quarterTurn, readFace, repeatedFaces, RobotApi, RobotError, RobotRun, rotateSquare, squareCrop,
} from "../js/robot.js";
import { picture, quad, rng } from "./pictures.mjs";
import { readPng, shrink } from "./png.mjs";
import { json, mockServer, PHOTOS } from "./robot-mock.mjs";

// ------------------------------------------------------------- pictures

// A picture whose every pixel says where it was: 1 + its index in its red,
// green and blue bytes, opaque.
function numbered(W, H) {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let i = 0; i < W * H; i++) data.set([(i + 1) & 255, (i + 1) >> 8, 0, 255], i * 4);
  return { width: W, height: H, data };
}
const pixel = (image, x, y) => new Uint32Array(image.data.buffer)[y * image.width + x] & 0xffffff;

// The page's grab without a canvas: the centred square, made `frameSize`
// small, turned (grabFrame does the middle step with drawImage).
function grab(frame, rotate, n) {
  const { sx, sy, side } = squareCrop(frame.width, frame.height);
  const size = Math.min(side, frameSize(n));
  const square = shrink(frame, sx, sy, side, size);
  return { width: size, height: size, data: rotateSquare(square.data, size, rotate) };
}

// What the camera sees when the robot holds up a face that, turned
// `rotate` clockwise, reads the way its facelets are written: the face's
// picture turned back, in the middle of a wider frame.
function cameraFrame(face, rotate, W = 320) {
  const side = face.width;
  const turned = rotateSquare(face.data, side, (360 - rotate) % 360);
  const H = side;
  const data = new Uint8ClampedArray(W * H * 4);
  for (let i = 0; i < W * H; i++) data.set([60, 64, 70, 255], i * 4);
  const x0 = Math.floor((W - side) / 2);
  for (let y = 0; y < H; y++) data.set(turned.subarray(y * side * 4, (y + 1) * side * 4), (y * W + x0) * 4);
  return { width: W, height: H, data };
}

// The six faces of a cube (facelets 0..5 per sticker, URFDLB) as pictures
// the robot's camera takes: each face square to the camera, filling most of it.
function facePictures(n, facelets, side = 240) {
  const nn = n * n;
  return [0, 1, 2, 3, 4, 5].map((f) =>
    picture({ W: side, H: side, n, corners: quad(side / 2, side / 2, side * 0.72, 1.5, 0.02, 0.01), stickers: Array.from(facelets.slice(f * nn, (f + 1) * nn)), seed: 7 + f }),
  );
}

// ------------------------------------------------------------ geometry

test("squareCrop: the centred square, rounded down as robot.py's integer division", () => {
  assert.deepEqual(squareCrop(1280, 720), { sx: 280, sy: 0, side: 720 });
  assert.deepEqual(squareCrop(1920, 1080), { sx: 420, sy: 0, side: 1080 });
  assert.deepEqual(squareCrop(7, 4), { sx: 1, sy: 0, side: 4 });
  assert.deepEqual(squareCrop(5, 8), { sx: 0, sy: 1, side: 5 });
  assert.deepEqual(squareCrop(640, 640), { sx: 0, sy: 0, side: 640 });
});

test("rotateSquare: clockwise quarter turns as cv2.rotate does them", () => {
  // [a b; c d] with a=1, b=2, c=3, d=4.
  const square = numbered(2, 2);
  const turned = (r) => Array.from(new Uint32Array(rotateSquare(square.data, 2, r).buffer), (v) => v & 0xffffff);
  assert.deepEqual(turned(0), [1, 2, 3, 4]);
  assert.deepEqual(turned(90), [3, 1, 4, 2]); // ROTATE_90_CLOCKWISE
  assert.deepEqual(turned(180), [4, 3, 2, 1]); // ROTATE_180
  assert.deepEqual(turned(270), [2, 4, 1, 3]); // ROTATE_90_COUNTERCLOCKWISE
  assert.deepEqual(turned(-90), turned(270));
  assert.deepEqual(turned(450), turned(90));
  assert.throws(() => rotateSquare(square.data, 2, 45));
  // Four quarter turns are none; a quarter turn and three more likewise.
  const big = numbered(9, 9);
  let data = big.data;
  for (let k = 0; k < 4; k++) data = rotateSquare(data, 9, 90);
  assert.deepEqual(data, big.data);
  assert.deepEqual(rotateSquare(rotateSquare(big.data, 9, 90), 9, 270), big.data);
  assert.deepEqual(rotateSquare(rotateSquare(big.data, 9, 180), 9, 180), big.data);
});

test("a frame grabbed: the corner of the square comes round where a clockwise turn puts it", () => {
  const frame = numbered(16, 9); // a 16:9 camera picture
  const { sx, sy, side } = squareCrop(16, 9);
  assert.deepEqual({ sx, sy, side }, { sx: 3, sy: 0, side: 9 });
  const corner = pixel(frame, sx, sy); // top left of the square
  const right = pixel(frame, sx + side - 1, sy); // its top right
  const at = (r) => {
    const g = grab(frame, r, 3);
    return [pixel(g, 0, 0), pixel(g, side - 1, 0), pixel(g, side - 1, side - 1), pixel(g, 0, side - 1)];
  };
  assert.equal(at(0)[0], corner);
  assert.equal(at(90)[1], corner); // top left to top right
  assert.equal(at(180)[2], corner); // to bottom right
  assert.equal(at(270)[3], corner); // to bottom left
  assert.equal(at(90)[2], right); // top right to bottom right
  assert.equal(quarterTurn("270"), 270);
  assert.equal(quarterTurn(undefined), 0);
  assert.equal(quarterTurn(30), null);
});

test("gridCells reads a centred grid of 0.8 of the shorter side, as anycube_scan.mjs", () => {
  assert.equal(GRID_SHARE, 0.8);
  const n = 3;
  const W = 300;
  const H = 200;
  const side = H * 0.8;
  const step = side / n;
  const data = new Uint8ClampedArray(W * H * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const c = Math.floor((x - (W - side) / 2) / step);
      const r = Math.floor((y - (H - side) / 2) / step);
      const inside = c >= 0 && c < n && r >= 0 && r < n;
      data.set(inside ? [r * 80 + 20, c * 80 + 20, 128, 255] : [0, 0, 0, 255], (y * W + x) * 4);
    }
  }
  const cells = gridCells({ width: W, height: H, data }, n);
  assert.equal(cells.length, 9);
  // Each cell read in its own middle: its own colour, nothing of its neighbours'.
  cells.forEach(([r, g, b], k) => {
    assert.deepEqual([Math.round(r), Math.round(g), Math.round(b)], [Math.floor(k / n) * 80 + 20, (k % n) * 80 + 20, 128]);
  });
  // Nothing like a face in the picture: readFace falls back to the same grid.
  const random = rng(5);
  const mud = { width: W, height: H, data: new Uint8ClampedArray(W * H * 4).map((_, i) => (i % 4 === 3 ? 255 : 100 + random() * 30)) };
  const read = readFace(mud, n);
  assert.equal(read.source.found, false);
  assert.deepEqual(read.cells, gridCells(mud, n));
});

// ------------------------------------------------------------ the faces

const SCRAMBLE = "R U F' L2 D B' R2 U'";

function scrambled(n, moves = SCRAMBLE) {
  const model = new CubeModel(n);
  const parsed = model.parse(moves);
  const facelets = model.applyAll(model.solved(), parsed);
  return { model, facelets, text: model.toText(facelets), solution: model.names(parsed.map((m) => model.inverse(m)).reverse()).split(" ") };
}

test("six pictures of a scrambled cube read back as that cube", (t) => {
  for (const n of [2, 3, 4, 5]) {
    const { facelets, text } = scrambled(n);
    const frames = facePictures(n, facelets, frameSize(n));
    const began = performance.now();
    const read = frames.map((f) => readFace(f, n));
    const result = assembleFaces(n, read.flatMap((r) => r.cells));
    const ms = performance.now() - began;
    assert.ok(read.every((r) => r.source.found), `${n}: every face found`);
    assert.equal(result.ok, true, `${n}: ${result.note}`);
    // Every sticker the colour it was drawn; on an odd cube the very facelets
    // (an even one is named from its corners, in whatever way it is held).
    assert.deepEqual(Array.from(result.facelets, (k) => result.palette[k]), Array.from(facelets, (k) => STICKERS[k]));
    if (n % 2) assert.equal(result.state, text);
    t.diagnostic(`${n}x${n}x${n}: six ${frameSize(n)}-pixel frames read and assembled in ${ms.toFixed(1)} ms`);
  }
});

// The robot's own six pictures of a 4x4x4 (MultiCuber's CubeScan), when
// that repository is beside this one: as robot.py saved them, its U and D
// a half turn round (MultiCuber's README reads them with {"U":2,"D":2}).
const CUBESCAN = process.env.MULTICUBER_CUBESCAN ?? join(import.meta.dirname, "../../../MultiCuber/CubeScan");
const haveCubeScan = existsSync(join(CUBESCAN, "rubiks-side-U.png"));
// What anycube_scan.mjs reads from them (the same cube MultiCuber's tests solve).
const CUBESCAN_STATE = "UUUUUUUUUUUURRRRDRRRDRRRDRRRDRRRFFFFFFFFFFFFFFFFLLLLDDDDDDDDDDDDLLLULLLULLLULLLUBBBBBBBBBBBBBBBB";

function cubeScanFaces() {
  return [..."URFDLB"].map((f) => {
    const p = readPng(join(CUBESCAN, `rubiks-side-${f}.png`));
    return "UD".includes(f) ? { ...p, data: rotateSquare(p.data, p.width, 180) } : p;
  });
}

test("the robot's own pictures of a 4x4x4: read in a few tens of ms", { skip: !haveCubeScan && "MultiCuber/CubeScan not found" }, (t) => {
  const faces = cubeScanFaces();
  let best = Infinity;
  let result;
  let split;
  for (let k = 0; k < 5; k++) {
    const frames = faces.map((f) => grab(f, 0, 4));
    const t0 = performance.now();
    const read = frames.map((f) => readFace(f, 4));
    const t1 = performance.now();
    result = assembleFaces(4, read.flatMap((r) => r.cells));
    const t2 = performance.now();
    if (t2 - t0 < best) [best, split] = [t2 - t0, [t1 - t0, t2 - t1]];
    assert.ok(read.every((r) => r.source.found));
  }
  assert.equal(result.ok, true, result.note);
  assert.equal(result.state, CUBESCAN_STATE);
  t.diagnostic(`six 1080-pixel pictures at ${frameSize(4)} pixels: faces ${split[0].toFixed(1)} ms, assembly ${split[1].toFixed(1)} ms, ${best.toFixed(1)} ms in all`);
});

test("repeatedFaces: two photos of one picture, not two faces alike", () => {
  const { facelets } = scrambled(3);
  const read = facePictures(3, facelets).map((f) => readFace(f, 3).cells);
  assert.deepEqual(repeatedFaces(read), []);
  const noisy = read[2].map((c) => c.map((v) => v + 4));
  assert.deepEqual(repeatedFaces([...read.slice(0, 5), noisy]), [[2, 5]]);
  // A solved cube's faces are each one colour, but six different ones.
  const solved = facePictures(3, new CubeModel(3).solved()).map((f) => readFace(f, 3).cells);
  assert.deepEqual(repeatedFaces(solved), []);
});

// --------------------------------------------------------- time left

test("Eta: the plan's estimate, then corrected by how the ops went", () => {
  const plan = { eta_ms: 1000, ops: [{ est_ms: 200 }, { est_ms: 300 }, { est_ms: 500 }] };
  const eta = new Eta(plan, 0);
  assert.equal(eta.left(0), 1000);
  assert.equal(eta.left(400), 600);
  eta.op(0, 400, 400); // twice as slow as estimated
  assert.equal(eta.left(400), 1600); // 800 left, at twice the estimate
  eta.op(1, 300, 700); // 700 for 500
  assert.equal(Math.round(eta.left(700)), 700); // 500 left at 1.4
  eta.op(1, 300, 900); // the same op again is passed over
  assert.equal(Math.round(eta.left(700)), 700);
  eta.op(2, 500, 1200);
  assert.equal(eta.left(1200), 0);
  // Without eta_ms, the sum of the ops; ratios held within 0.5 and 2.
  const fast = new Eta({ ops: [{ est_ms: 1000 }, { est_ms: 1000 }] }, 0);
  assert.equal(fast.left(0), 2000);
  fast.op(0, 10, 10);
  assert.equal(fast.left(10), 500);
});

// ------------------------------------------------------------ the API

test("RobotApi: answers, refusals and no server, each as said", async () => {
  const seen = [];
  const api = new RobotApi(async (url, init) => {
    seen.push([init.method, url, init.body && JSON.parse(init.body)]);
    if (url === "/api/robot/connect" && JSON.parse(init.body).port) return json(200, { ok: false, error: { code: "no_port", message: "No robot on that port." } });
    if (url === "/api/robot/scan") return json(500, null);
    if (url === "/api/robot/stop") throw new TypeError("Failed to fetch");
    if (url === "/api/robots") return json(200, { drivers: [], connected: null });
    return json(200, { ok: true });
  });
  assert.deepEqual(await api.robots(), { drivers: [], connected: null });
  await assert.rejects(api.connect("multicuber", "/dev/x"), (e) => e instanceof RobotError && e.code === "no_port" && e.message === "No robot on that port.");
  await assert.rejects(api.scan(), (e) => e.code === "http_500");
  await assert.rejects(api.stop(), (e) => e.code === "network");
  await api.solve("r1", 3, "UUU", 12);
  await api.connect("multicuber");
  assert.deepEqual(seen[1], ["POST", "/api/robot/connect", { driver: "multicuber", port: "/dev/x" }]);
  assert.deepEqual(seen[4], ["POST", "/api/robot/solve", { run: "r1", size: 3, facelets: "UUU", analysis_ms: 12 }]);
  assert.deepEqual(seen[5], ["POST", "/api/robot/connect", { driver: "multicuber" }]);
});

// A run followed to its end: everything the hooks heard, in order.
function follow(server, n, frames, { until = "done" } = {}) {
  const heard = [];
  let finish;
  const over = new Promise((resolve) => (finish = resolve));
  const robot = new RobotRun({
    api: new RobotApi(server.fetch),
    n,
    wait: 1,
    grab: (rotate) => frames(rotate),
    hooks: {
      onphase: (p) => {
        heard.push(["phase", p]);
        if (p === "idle" && until === "idle") finish();
      },
      onscanstart: (e) => heard.push(["scan-start", e.steps]),
      onphoto: (index, face) => heard.push(["photo", index, face]),
      onface: (face, cells, source) => heard.push(["face", face, source.found]),
      onscanned: (r) => heard.push(["scanned", r]),
      onplan: (p) => heard.push(["plan", p.moves.length]),
      onmove: (e) => heard.push(["move", e.move, e.est_ms]),
      onop: (e) => heard.push(["op", e.i]),
      ondone: (e) => {
        heard.push(["done", e.times]);
        finish();
      },
      onerror: (e) => {
        heard.push(["error", e.code, e.message]);
        if (until === "error") finish();
      },
    },
  });
  return { robot, heard, over };
}

test("a whole run against the mock: six photos, the cube read, solved in step with the robot", async (t) => {
  const n = 3;
  const { facelets, text, solution } = scrambled(n);
  const pictures = facePictures(n, facelets);
  const server = mockServer({ moves: solution });
  // The camera: whatever face the robot is holding, as the camera sees it.
  let holding = 0;
  const frames = (rotate) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, n);
  };
  const { robot, heard, over } = follow(server, n, frames);
  await robot.follow();
  await robot.scan();
  await over;
  robot.unfollow();

  const of = (kind) => heard.filter((h) => h[0] === kind);
  assert.deepEqual(of("photo").map((h) => h[2]), [0, 1, 3, 4, 5, 2]); // U R D L B F as facelet faces
  assert.equal(of("face").length, 6);
  assert.ok(of("face").every((h) => h[2]), "every face found");
  // Each photo was answered before the robot went on, with its run and index.
  const acks = server.calls.filter((c) => c.path === "/api/robot/photo-taken").map((c) => c.body);
  assert.deepEqual(acks, [0, 1, 2, 3, 4, 5].map((index) => ({ run: "run-1", index })));
  // The cube read is the cube scrambled; the server was asked to solve exactly that.
  const [, scanned] = of("scanned")[0];
  assert.equal(scanned.ok, true, scanned.note);
  assert.equal(scanned.state, text);
  assert.ok(Number.isInteger(scanned.analysis_ms) && scanned.analysis_ms >= 0);
  const solve = server.calls.find((c) => c.path === "/api/robot/solve").body;
  assert.deepEqual(solve, { run: "run-1", size: 3, facelets: text, analysis_ms: scanned.analysis_ms });
  // The plan, every move in order with its time, every op, then done.
  assert.equal(of("plan")[0][1], solution.length);
  assert.deepEqual(of("move").map((h) => h[1]), solution.map((_, k) => k));
  assert.ok(of("move").every((h) => h[2] > 0));
  assert.equal(of("op").length, solution.length * 2);
  assert.deepEqual(of("phase").map((h) => h[1]), ["starting", "scanning", "solving", "done"]);
  assert.equal(of("done")[0][1].analysis_ms, scanned.analysis_ms);
  t.diagnostic(`scan-done to facelets: ${scanned.analysis_ms} ms; reading the six faces and assembling: ${scanned.work_ms} ms of work`);
});

test("the robot's own pictures through a whole scan: photos turned as robot.py turns them", { skip: !haveCubeScan && "MultiCuber/CubeScan not found" }, async (t) => {
  const faces = cubeScanFaces();
  const server = mockServer({ moves: ["R'"] });
  let holding = 0;
  const frames = (rotate) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(faces["URFDLB".indexOf(face)], turn, 1920), rotate, 4);
  };
  const { robot, heard, over } = follow(server, 4, frames);
  await robot.follow();
  await robot.scan();
  await over;
  robot.unfollow();
  const [, scanned] = heard.find((h) => h[0] === "scanned");
  assert.equal(scanned.ok, true, scanned.note);
  // One turn of R from solved: R' solves it.
  // The same cube as the pictures read as saved: each frame came round right.
  assert.equal(scanned.state, CUBESCAN_STATE);
  t.diagnostic(`scan-done to facelets: ${scanned.analysis_ms} ms; ${scanned.work_ms} ms of work for six faces`);
});

test("a scan that makes no cube: said, and the robot is asked nothing more", async () => {
  const server = mockServer({ moves: [] });
  // A scrambled cube with its top shown in place of its bottom: no cube has that.
  const pictures = facePictures(3, scrambled(3).facelets);
  let holding = 0;
  const frames = (rotate) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face === "D" ? "U" : face)], turn), rotate, 3);
  };
  const { robot, heard, over } = follow(server, 3, frames, { until: "idle" });
  await robot.follow();
  await robot.scan();
  await over;
  robot.unfollow();
  const [, scanned] = heard.find((h) => h[0] === "scanned");
  assert.equal(scanned.ok, false, `${scanned.kind} ${scanned.state} ${scanned.note}`);
  assert.equal(scanned.state, null);
  assert.ok(scanned.note);
  assert.equal(server.calls.filter((c) => c.path === "/api/robot/solve").length, 0);
  // The cause is plain to see: the top's photo taken twice.
  assert.deepEqual(scanned.repeated, [[0, 3]]);
  assert.equal(robot.phase, "idle");
  assert.equal(robot.busy, false);
});

test("an error from the stream ends the run, and the next scan starts afresh", async () => {
  const n = 3;
  const { facelets, solution } = scrambled(n);
  const pictures = facePictures(n, facelets);
  const server = mockServer({ moves: solution, failAfterPhoto: 2 });
  let holding = 0;
  const frames = (rotate) => {
    const [face, turn] = PHOTOS[holding++ % 6];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, n);
  };
  const first = follow(server, n, frames, { until: "error" });
  await first.robot.follow();
  await first.robot.scan();
  await first.over;
  assert.deepEqual(first.heard.find((h) => h[0] === "error"), ["error", "jam", "The lift did not reach its position."]);
  assert.equal(first.robot.phase, "failed");
  first.robot.unfollow();
  // Faces read for the failed run come to nothing; a new run is whole.
  const second = mockServer({ moves: solution });
  holding = 0;
  const again = follow(second, n, frames);
  await again.robot.follow();
  await again.robot.scan();
  await again.over;
  again.robot.unfollow();
  assert.ok(again.heard.some((h) => h[0] === "done"));
});

test("no picture from the camera: the robot is stopped, not left waiting", async () => {
  const server = mockServer({ moves: [] });
  const { robot, heard, over } = follow(server, 3, () => null, { until: "error" });
  await robot.follow();
  await robot.scan();
  await over;
  await new Promise((resolve) => setTimeout(resolve, 20));
  robot.unfollow();
  assert.equal(heard.find((h) => h[0] === "error")[1], "no_frame");
  assert.ok(server.calls.some((c) => c.path === "/api/robot/stop"));
  assert.ok(!server.calls.some((c) => c.path === "/api/robot/photo-taken"));
});

test("the server unreachable: a failed scan is said, the events wait and come back", async () => {
  let down = true;
  const server = mockServer({ moves: [], down: (path) => down && path !== "/api/robot/events" });
  const links = [];
  const errors = [];
  const robot = new RobotRun({
    api: new RobotApi(server.fetch),
    n: 3,
    wait: 1,
    grab: () => null,
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms / 100)),
    hooks: { onerror: (e) => errors.push(e), onlink: (ok) => links.push(ok) },
  });
  await robot.follow();
  await robot.scan();
  assert.equal(errors[0].code, "network");
  assert.equal(robot.phase, "failed");
  assert.equal(robot.busy, false);
  // The event stream lost and found again.
  down = false;
  const realFetch = server.fetch;
  let lost = true;
  robot.api = new RobotApi(async (url, init) => {
    if (lost && url.startsWith("/api/robot/events")) throw new TypeError("Failed to fetch");
    return realFetch(url, init);
  });
  await new Promise((resolve) => setTimeout(resolve, 120));
  lost = false;
  await new Promise((resolve) => setTimeout(resolve, 150));
  robot.unfollow();
  assert.deepEqual(links, [false, true]);
});

test("events that come before the scan's answer are kept for its run; others' are passed over", async () => {
  const heard = [];
  let answer;
  const api = {
    scan: () => new Promise((resolve) => (answer = resolve)),
    photoTaken: async () => ({ ok: true }),
    events: async () => ({ seq: 0, events: [] }),
  };
  const robot = new RobotRun({ api, n: 3, grab: () => ({ width: 1, height: 1, data: new Uint8ClampedArray(4) }), later: () => {}, hooks: { onscanstart: (e) => heard.push(e.run), onphoto: (i) => heard.push(i) } });
  const scanning = robot.scan();
  robot.take({ seq: 1, type: "scan-start", run: "old", steps: 1 });
  robot.take({ seq: 2, type: "scan-start", run: "new", steps: 1 });
  robot.take({ seq: 3, type: "photo", run: "new", index: 0, face: "U", rotate: 0 });
  answer({ ok: true, run: "new" });
  await scanning;
  robot.take({ seq: 4, type: "scan-start", run: "old", steps: 1 });
  assert.deepEqual(heard, ["new", 0]);
});
