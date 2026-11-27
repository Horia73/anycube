// The robot extension's core (js/robot.js): the frame turned,
// the faces read, the time left, the server's API, and whole
// runs against a mock of the server whose events are scripted the way the
// robot behaves (robots-spec/API.md): it waits for each photo-taken.

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { after, test } from "node:test";
import { CameraFeed } from "../js/camera.js";
import { STICKERS } from "../js/colour.js";
import { CubeModel } from "../js/cube-model.js";
import {
  assembleFaces, bindInfo, choosePort, Eta, frameSize, portName, quarterTurn, readCost, readFace, ReadPace, readSizes, repeatedFaces, RobotApi, RobotError, RobotRun,
  RobotWatch, robotChoices, rotateSquare, scanMeasures,
} from "../js/robot.js";
import { picture, quad } from "./pictures.mjs";
import { readPng, shrink } from "./png.mjs";
import { HOST, json, mockServer, PHOTOS } from "./robot-mock.mjs";

// ------------------------------------------------------------- pictures

// A picture whose every pixel says where it was: 1 + its index in its red,
// green and blue bytes, opaque.
function numbered(W, H) {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let i = 0; i < W * H; i++) data.set([(i + 1) & 255, (i + 1) >> 8, 0, 255], i * 4);
  return { width: W, height: H, data };
}
// Square fixture for the controller/pace tests. Browser capture is exercised
// separately using a video element and an encoded controller PNG.
function grab(frame, rotate, size) {
  const side = frame.width;
  size = Math.min(side, size);
  const square = shrink(frame, 0, 0, side, size);
  return { width: size, height: size, data: rotateSquare(square.data, size, rotate) };
}

// What the camera sees when the robot holds up a face that, turned
// `rotate` clockwise, reads the way its facelets are written: the face's
// picture turned back before the photo request.
function cameraFrame(face, rotate) {
  return { ...face, data: rotateSquare(face.data, face.width, (360 - rotate) % 360) };
}

// The six faces of a cube (facelets 0..5 per sticker, URFDLB) as pictures
// the robot's camera takes: each face square to the camera, filling most of it.
function facePictures(n, facelets, side = 240, corners = quad(side / 2, side / 2, side * 0.72, 1.5, 0.02, 0.01)) {
  const nn = n * n;
  return [0, 1, 2, 3, 4, 5].map((f) =>
    picture({ W: side, H: side, n, corners, stickers: Array.from(facelets.slice(f * nn, (f + 1) * nn)), seed: 7 + f }),
  );
}

// ------------------------------------------------------------ geometry

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

test("photo quarter turns", () => {
  assert.equal(quarterTurn("270"), 270);
  assert.equal(quarterTurn(undefined), 0);
  assert.equal(quarterTurn(30), null);
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
    const frames = faces.map((f) => grab(f, 0, frameSize(4)));
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

// What goes on asking the server by itself: stopped when the tests are over,
// so that a test that fails is a test that failed, not a file that never ends.
const going = [];
const running = (thing) => (going.push(thing), thing);
after(() => {
  for (const thing of going) (thing.unfollow ?? thing.stop).call(thing);
});

// A run followed to its end: everything the hooks heard, in order.
function follow(server, n, frames, { until = "done", ...more } = {}) {
  const heard = [];
  let finish;
  const over = new Promise((resolve) => (finish = resolve));
  const robot = running(new RobotRun({
    api: new RobotApi(server.fetch),
    n,
    wait: 1,
    grab: (rotate, size) => frames(rotate, size),
    ...more,
    hooks: {
      onphase: (p) => {
        heard.push(["phase", p]);
        if (p === "idle") finish();
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
  }));
  return { robot, heard, over };
}

test("a whole run against the mock: six photos, the cube read, solved in step with the robot", async (t) => {
  const n = 3;
  const { facelets, text, solution } = scrambled(n);
  const pictures = facePictures(n, facelets, 240, quad(144, 103, 140, 19, 0.18, 0.11));
  const server = mockServer({ moves: solution });
  // The camera: whatever face the robot is holding, as the camera sees it.
  let holding = 0;
  const frames = (rotate, size) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, size);
  };
  const { robot, heard, over } = follow(server, n, frames);
  await robot.follow();
  await robot.scan();
  await over;
  robot.unfollow();

  const of = (kind) => heard.filter((h) => h[0] === kind);
  assert.deepEqual(of("photo").map((h) => h[2]), [4, 0, 1, 5, 3, 2]); // L U R B D F as facelet faces
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
  const frames = (rotate, size) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(faces["URFDLB".indexOf(face)], turn), rotate, size);
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

test("capture and acknowledgement precede analysis, and a missed face prevents solving", async () => {
  const n = 3;
  const pictures = facePictures(n, scrambled(n).facelets);
  const pending = [];
  const scans = [];
  const acks = [];
  let solves = 0;
  let capture;
  let acknowledge;
  const robot = new RobotRun({
    n,
    api: {
      scan: async () => ({ run: "scan" }),
      photoTaken: (run, index) => {
        acks.push([run, index]);
        return new Promise((resolve) => { acknowledge = resolve; });
      },
      solve: async () => { solves++; },
    },
    grab: () => new Promise((resolve) => { capture = resolve; }),
    later: (read) => pending.push(read),
    hooks: { onscanned: (result) => scans.push(result) },
  });
  await robot.scan();

  for (const [index, [face, rotate]] of PHOTOS.entries()) {
    const photo = robot.take({ type: "photo", run: "scan", index, face, rotate });
    assert.equal(acks.length, index);
    assert.equal(pending.length, index);
    capture(face === "R" ? { width: 240, height: 240, data: new Uint8ClampedArray(240 * 240 * 4) } : pictures["URFDLB".indexOf(face)]);
    await Promise.resolve();
    assert.deepEqual(acks.at(-1), ["scan", index]);
    assert.equal(pending.length, index);
    acknowledge();
    await photo;
  }

  robot.take({ type: "scan-done", run: "scan" });
  for (const read of pending.slice(0, -1)) read();
  assert.deepEqual(scans, []);
  pending.at(-1)();
  assert.equal(scans.length, 1);
  const [result] = scans;
  assert.equal(result.ok, false);
  assert.equal(result.kind, "detection");
  assert.equal(result.state, null);
  assert.match(result.note, /photos for R\./);
  assert.equal(result.sources.filter((source) => source.found).length, 5);
  assert.equal(solves, 0);
  assert.equal(robot.phase, "idle");
});

test("a scan that makes no cube: said, and the robot is asked nothing more", async () => {
  const server = mockServer({ moves: [] });
  // A scrambled cube with its top shown in place of its bottom: no cube has that.
  const pictures = facePictures(3, scrambled(3).facelets);
  let holding = 0;
  const frames = (rotate, size) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face === "D" ? "U" : face)], turn), rotate, size);
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
  const frames = (rotate, size) => {
    const [face, turn] = PHOTOS[holding++ % 6];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, size);
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
  const robot = running(new RobotRun({
    api: new RobotApi(server.fetch),
    n: 3,
    wait: 1,
    grab: () => null,
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms / 100)),
    hooks: { onerror: (e) => errors.push(e), onlink: (ok) => links.push(ok) },
  }));
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
  await new Promise(setImmediate);
  robot.take({ seq: 4, type: "scan-start", run: "old", steps: 1 });
  assert.deepEqual(heard, ["new", 0]);
});

// ------------------------------------------------------- the size read at

test("ReadPace: the largest size that is read before the next photo, a size down only when it would not be", () => {
  assert.deepEqual(readSizes(3), [240, 320, 360, 400, 480, 560, 640, 720]);
  assert.equal(readSizes(7)[0], 360);
  const top = readSizes(3).at(-1);
  const rate = (size, ms) => ms / readCost(size);
  // Twice the side is less than four times the time.
  assert.ok(readCost(480) / readCost(240) > 2 && readCost(480) / readCost(240) < 4);

  // Nothing measured yet: a size in the middle, and the least for a last photo (which nothing waits behind).
  const fresh = new ReadPace(3);
  assert.equal(fresh.photo(0), 480);
  assert.equal(new ReadPace(3).photo(0, true), 240);
  assert.equal(new ReadPace(3, { first: 100 }).photo(0), 240);
  // Once a face was read, as large as there is time for.
  fresh.took(480, 30);
  assert.equal(fresh.photo(1000), top);

  // 72 ms for the largest, 800 ms to the next photo (as long as none was seen), half of it to spend: the largest.
  const pace = new ReadPace(3);
  pace.took(720, 72);
  assert.equal(pace.photo(0), 720);
  // The last photo has 25 ms: the largest size read in that.
  assert.equal(pace.fit(25), 320);
  assert.ok(rate(720, 72) * readCost(320) <= 25 && rate(720, 72) * readCost(360) > 25);
  // A reading that was slower counts at once; one that was quicker by halves.
  pace.took(720, 600);
  assert.equal(pace.rate, rate(720, 600));
  assert.equal(pace.fit(400), 480); // 560 would take 411 ms
  pace.took(560, 36);
  assert.equal(pace.rate, (rate(720, 600) + rate(560, 36)) / 2);
  // However slow, never under the least size.
  pace.took(240, 5000);
  assert.equal(pace.fit(400), 240);

  // The photos of a scan say how long there is: here 100 ms from one to the next.
  const quick = new ReadPace(3);
  quick.took(720, 72);
  quick.begin();
  assert.equal(quick.photo(1000), 720); // nothing seen yet: 800 ms taken on trust
  assert.equal(quick.photo(1100), 560); // 100 ms seen: 50 to spend, and 640 would take 60
  assert.equal(quick.photo(1300), 560); // the shortest seen counts
  assert.equal(quick.photo(1400, true), 320); // the last: 25 ms
  // The next scan expects what this one showed, from its first photo on.
  quick.begin();
  assert.equal(quick.gap, 100);
  assert.equal(quick.photo(5000), 560);

  // The server's own word for the time to the next photo is taken before any of that.
  assert.equal(quick.photo(5100, false, 2000), 720);
  assert.equal(quick.photo(5200, true, 300), 720); // also after the last, when the robot is still busy
  assert.equal(quick.photo(5300, true, 0), 320); // nothing left of the robot's time: the 25 ms all the same

  // A larger cube has a larger least size.
  quick.begin(7);
  assert.equal(quick.fit(0), 360);
});

test("a scan reads its photos as large as the robot's pace allows, and the last one small", async (t) => {
  const n = 3;
  const { facelets, text } = scrambled(n);
  const pictures = facePictures(n, facelets, 720);
  // How long reading takes is not this machine's to say here: the run is given
  // a clock on which reading a face takes exactly what its size costs (72 ms
  // for the largest, as in the test of ReadPace above) and nothing else any time.
  const cost = (size) => (72 / readCost(720)) * readCost(size);
  let clock = 0;
  let reading = null;
  const now = () => {
    // Within a reading the clock is asked before it and after it, and the second answer is later by the cost.
    if (reading && reading.asked++ === 1) clock += cost(reading.size);
    return clock;
  };
  const scan = async (script, pace) => {
    const server = mockServer({ moves: [], ...script });
    let holding = 0;
    const asked = [];
    const frames = (rotate, size) => {
      const [face, turn] = PHOTOS[holding++];
      asked.push(size);
      return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, size);
    };
    // A photo is read right after it was grabbed: the size last asked for is the size read.
    const later = (read) => {
      const size = asked.at(-1);
      setTimeout(() => {
        reading = { size, asked: 0 };
        try {
          read();
        } finally {
          reading = null;
        }
      }, 0);
    };
    const { robot, heard, over } = follow(server, n, frames, { until: "idle", pace, now, later });
    await robot.follow();
    await robot.scan();
    await Promise.race([over, new Promise((resolve) => setTimeout(resolve, 20_000))]);
    robot.unfollow();
    const [, scanned] = heard.find((h) => h[0] === "scanned");
    assert.equal(scanned.state, text, scanned.note);
    // Each photo was read at the size it was grabbed at, and its time kept.
    assert.deepEqual(scanned.sizes, asked);
    assert.deepEqual(scanned.read_ms, asked.map((size) => Math.round(cost(size) * 10) / 10));
    t.diagnostic(`sizes ${scanned.sizes.join(" ")}; ms a face ${scanned.read_ms.map(Math.round).join(" ")}`);
    return scanned;
  };
  // A robot that says it takes a minute from photo to photo: the first photo
  // of all in the middle (nothing is known of this machine), then the largest size.
  const pace = new ReadPace(n);
  assert.deepEqual((await scan({ nextMs: 60_000 }, pace)).sizes, [480, 720, 720, 720, 720, 720]);
  // What was learnt is kept: the next scan is at the largest from its first photo.
  assert.deepEqual((await scan({ nextMs: 60_000 }, pace)).sizes, [720, 720, 720, 720, 720, 720]);
  // A robot that says it leaves no time at all: the least size, and for the
  // last photo what 25 ms allow (320: at this rate 360 would take 25.5 ms).
  assert.ok(cost(320) <= 25 && cost(360) > 25);
  assert.deepEqual((await scan({ nextMs: 0 }, pace)).sizes, [240, 240, 240, 240, 240, 320]);
  // A robot that says nothing of its time, on a machine of which nothing is
  // known: the first photo in the middle, the last at what 25 ms allow. (The
  // photos between go by the time the mock really took from one to the next.)
  const hurried = (await scan({}, new ReadPace(n))).sizes;
  assert.equal(hurried[0], 480);
  assert.equal(hurried[5], 320);
});

// ---------------------------------------------------------- plug and play

const PORT = "/dev/cu.usbmodem21301";
const ROBOT = { id: "multicuber", name: "MultiCuber", sizes: [3], ports: [PORT] };

test("choosePort: the robot that is connected without asking", () => {
  assert.equal(portName(PORT), "cu.usbmodem21301");
  assert.equal(choosePort({ drivers: [{ ...ROBOT, ports: [] }], connected: null }), null);
  assert.equal(choosePort({}), null);
  // A server that only names its ports: the port its one driver claims.
  assert.deepEqual(choosePort({ drivers: [ROBOT] }), { driver: "multicuber", port: PORT, sure: false });
  // Two drivers claim it: not without knowing which it was last time.
  const other = { id: "other", name: "Other", ports: [PORT, "/dev/cu.other"] };
  assert.deepEqual(choosePort({ drivers: [ROBOT, other] }), { driver: "other", port: "/dev/cu.other", sure: false });
  assert.equal(choosePort({ drivers: [ROBOT, { ...other, ports: [PORT] }] }), null);
  assert.deepEqual(choosePort({ drivers: [ROBOT, other] }, { driver: "multicuber", port: PORT }), { driver: "multicuber", port: PORT, sure: false });
  // The robot of last time before another; what is skipped is not chosen.
  const two = { drivers: [{ ...ROBOT, ports: ["/dev/a", "/dev/b"] }] };
  assert.equal(choosePort(two, { driver: "multicuber" }).port, "/dev/a"); // last time's port was never said
  assert.equal(choosePort(two).port, "/dev/a");
  assert.equal(choosePort(two, { driver: "multicuber", port: "/dev/b" }).port, "/dev/b");
  assert.equal(choosePort(two, null, (driver, port) => port === "/dev/a").port, "/dev/b");

  // A server that says what its ports are: only the board that answered as a robot.
  const said = (ports) => ({ drivers: [{ ...ROBOT, ports: ports.map((p) => p.path) }], ports });
  assert.deepEqual(choosePort(said([{ path: "/dev/a", driver: null }, { path: "/dev/b", driver: "multicuber" }])), { driver: "multicuber", port: "/dev/b", sure: true });
  assert.equal(choosePort(said([{ path: "/dev/a", driver: null }])), null); // answered as no robot
  assert.equal(choosePort(said([{ path: "/dev/a", driver: "multicuber", busy: true }])), null); // somebody else's
  assert.equal(choosePort(said([{ path: "/dev/a", driver: "unheard-of" }])), null);
  // Not looked at yet (no `driver` said): by its name, as with a server that says nothing.
  assert.deepEqual(choosePort(said([{ path: "/dev/a", busy: false }])), { driver: "multicuber", port: "/dev/a", sure: false });
  assert.equal(choosePort(said([{ path: "/dev/a", busy: true }])), null);
  // Recognised beats named, even against last time's port; recognised without any driver claiming the name too.
  assert.equal(choosePort(said([{ path: "/dev/a" }, { path: "/dev/b", driver: "multicuber" }]), { driver: "multicuber", port: "/dev/a" }).port, "/dev/b");
  assert.deepEqual(choosePort({ drivers: [{ ...ROBOT, ports: [] }], ports: [{ path: "/dev/z", driver: "multicuber" }] }), { driver: "multicuber", port: "/dev/z", sure: true });
});

// A watcher on a mock server: its looks are made by hand (the loop's own
// waiting never ends), its clock is `clock.t`.
function watcher(server, { auto = true, last = null } = {}) {
  const heard = [];
  const clock = { t: 0 };
  const memory = { auto, last };
  const watch = new RobotWatch({
    api: new RobotApi(server.fetch),
    memory,
    now: () => clock.t,
    sleep: () => new Promise(() => {}),
    hooks: {
      onconnecting: (r) => heard.push(["connecting", r.port]),
      onconnected: (r, how) => heard.push(["connected", r.port, how, r.size]),
      onfailed: (r, e) => heard.push(["failed", e.code, e.final]),
    },
  });
  const count = (path) => server.calls.filter((c) => c.path === path).length;
  return { watch, heard, clock, memory, count };
}

test("RobotWatch: a robot plugged in is connected, and the watching stops", async () => {
  for (const identify of [false, true]) {
    const server = mockServer({ ports: [], identify, size: 4 });
    const { watch, heard, memory, count } = watcher(server);
    watch.start();
    assert.equal(await watch.look(), "nothing");
    assert.equal(await watch.look(), "nothing");
    assert.equal(count("/api/robot/connect"), 0);
    server.plug();
    assert.equal(await watch.look(), "connected");
    assert.deepEqual(heard, [["connecting", PORT], ["connected", PORT, "plugged", 4]]);
    assert.deepEqual(server.calls.at(-1), { method: "POST", path: "/api/robot/connect", body: { driver: "multicuber", port: PORT } });
    assert.deepEqual(memory.last, { driver: "multicuber", port: PORT });
    assert.equal(watch.running, false);
  }
  // Connected for the cube the page says is in it.
  const server = mockServer({});
  const heard = [];
  const watch = new RobotWatch({ api: new RobotApi(server.fetch), memory: { auto: true, last: null }, size: (driver) => (driver === "multicuber" ? 5 : undefined), sleep: () => new Promise(() => {}), hooks: { onconnected: (r) => heard.push(r.size) } });
  watch.start();
  await watch.looking;
  assert.deepEqual(server.calls.at(-1).body, { driver: "multicuber", port: PORT, size: 5 });
  assert.deepEqual(heard, [5]);
});

test("RobotWatch: it asks every so often while it watches, and no more once a robot is connected", async () => {
  const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  // Waits for something to be so, however busy the machine is.
  const until = async (so, what) => {
    for (let k = 0; k < 2000 && !so(); k++) await pause(5);
    assert.ok(so(), what);
  };
  const server = mockServer({ ports: [] });
  const heard = [];
  const watch = running(new RobotWatch({ api: new RobotApi(server.fetch), memory: { auto: true, last: null }, every: 5, hooks: { onconnected: (r, how) => heard.push(how) } }));
  const asked = () => server.calls.filter((c) => c.path === "/api/robots").length;
  watch.start();
  await until(() => asked() >= 3, "asked again and again");
  assert.deepEqual(heard, []);
  server.plug();
  await until(() => heard.length > 0, "connected once plugged in");
  assert.deepEqual(heard, ["plugged"]);
  await watch.looking;
  const then = asked();
  await pause(60);
  assert.equal(asked(), then);
  // Stopped by hand: no more either.
  const idle = mockServer({ ports: [] });
  const second = running(new RobotWatch({ api: new RobotApi(idle.fetch), memory: { auto: true, last: null }, every: 5 }));
  second.start();
  await until(() => idle.calls.length >= 2, "asked");
  second.stop();
  await second.looking;
  const calls = idle.calls.length;
  await pause(60);
  assert.equal(idle.calls.length, calls);
});

test("RobotWatch: a board that is not a robot, or in use, is left alone; a robot the server holds is taken up", async () => {
  const server = mockServer({ ports: [], identify: true });
  const { watch, heard, count } = watcher(server);
  watch.start();
  server.plug("/dev/cu.arduino", { driver: null });
  server.plug("/dev/cu.printer", { busy: true, driver: "multicuber" });
  assert.equal(await watch.look(), "nothing");
  assert.equal(count("/api/robot/connect"), 0);
  server.plug("/dev/cu.robot", { driver: "multicuber" });
  assert.equal(await watch.look(), "connected");
  assert.deepEqual(heard.at(-1), ["connected", "/dev/cu.robot", "plugged", 3]);

  // Connect by itself switched off: one look, nothing connected; but a robot connected already is the page's.
  const off = mockServer({});
  const quiet = watcher(off, { auto: false });
  quiet.watch.start();
  await quiet.watch.looking;
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.equal(quiet.watch.running, false);
  assert.equal(quiet.count("/api/robots"), 1);
  assert.equal(quiet.count("/api/robot/connect"), 0);
  const holds = mockServer({ connected: { driver: "multicuber", port: PORT, size: 5, info: {} } });
  const taken = watcher(holds, { auto: false });
  taken.watch.start();
  await taken.watch.looking;
  assert.deepEqual(taken.heard, [["connected", PORT, "held", 5]]);
  assert.equal(taken.count("/api/robot/connect"), 0);
});

test("RobotWatch: a robot that does not connect is tried again later, and given up until plugged in anew", async () => {
  const server = mockServer({ connectFails: 2 });
  const { watch, heard, clock, count } = watcher(server);
  watch.start();
  assert.equal(await watch.looking, "failed");
  assert.deepEqual(heard.at(-1), ["failed", "no_answer", false]);
  // Not again at once, but after four seconds; then after eight more.
  assert.equal(await watch.look(), "nothing");
  clock.t = 3999;
  assert.equal(await watch.look(), "nothing");
  clock.t = 4000;
  assert.equal(await watch.look(), "failed");
  clock.t = 11_999;
  assert.equal(await watch.look(), "nothing");
  clock.t = 12_000;
  assert.equal(await watch.look(), "connected");
  assert.equal(count("/api/robot/connect"), 3);

  // One that never answers: five tries, then no more until it has been out and in again.
  const dead = mockServer({ connectFails: 1000 });
  const given = watcher(dead);
  given.watch.start();
  await given.watch.looking;
  for (let k = 0; k < 40; k++) {
    given.clock.t += 60_000;
    await given.watch.look();
  }
  assert.equal(given.count("/api/robot/connect"), 5);
  assert.deepEqual(given.heard.at(-1), ["failed", "no_answer", true]);
  dead.unplug();
  await given.watch.look();
  dead.plug();
  await given.watch.look();
  assert.equal(given.count("/api/robot/connect"), 6);
});

test("RobotWatch: disconnected by hand it stays so, until the cable has been out and in", async () => {
  const server = mockServer({});
  const { watch, heard, count } = watcher(server);
  watch.start();
  assert.equal(await watch.looking, "connected");
  // The page's Disconnect.
  await new RobotApi(server.fetch).disconnect();
  watch.hold("multicuber", PORT);
  watch.start();
  assert.equal(await watch.looking, "nothing");
  assert.equal(await watch.look(), "nothing");
  assert.equal(count("/api/robot/connect"), 1);
  server.unplug();
  assert.equal(await watch.look(), "nothing");
  server.plug();
  assert.equal(await watch.look(), "connected");
  assert.equal(heard.filter((h) => h[0] === "connected").length, 2);
  // A robot whose port was never said is held as a whole.
  await new RobotApi(server.fetch).disconnect();
  watch.hold("multicuber", "");
  watch.start();
  assert.equal(await watch.looking, "nothing");
  watch.release("multicuber", PORT);
  assert.equal(await watch.look(), "connected");
});

test("the robot unplugged: the run ends, the page hears it is gone, and it is connected again when it is back", async () => {
  const n = 3;
  const { facelets, solution } = scrambled(n);
  const pictures = facePictures(n, facelets);
  const server = mockServer({ moves: solution, stepMs: 5 });
  const { watch, heard: plugs } = watcher(server);
  watch.start();
  assert.equal(await watch.looking, "connected");

  let holding = 0;
  const frames = (rotate, size) => {
    const [face, turn] = PHOTOS[holding++];
    if (holding === 3) setTimeout(() => server.unplug(), 0); // the cable goes during the scan
    return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, size);
  };
  let gone;
  const lost = new Promise((resolve) => (gone = resolve));
  const { robot, heard } = follow(server, n, frames, { until: "error" });
  robot.hooks.onlost = (event) => gone(event);
  await robot.follow();
  await robot.scan();
  const event = await lost;
  robot.unfollow();
  // As the server says it: the run's own error first (code `disconnected`), then `disconnected` for the robot.
  assert.equal(event.type, "error");
  assert.equal(event.code, "disconnected");
  assert.deepEqual(server.events.slice(-2).map((e) => [e.type, e.run !== undefined]), [["error", true], ["disconnected", false]]);
  assert.deepEqual(heard.find((h) => h[0] === "error").slice(0, 2), ["error", "disconnected"]);
  assert.equal(robot.busy, false);
  assert.equal(server.connected, null);
  // Idle, it is heard just the same (no run to fail).
  const idle = new RobotRun({ api: new RobotApi(server.fetch), n, grab: () => null, hooks: { onlost: () => plugs.push(["lost"]), onerror: () => plugs.push(["error"]) } });
  idle.take({ seq: 99, type: "disconnected" });
  idle.take({ seq: 100, type: "error", code: "port_lost", message: "gone" });
  assert.deepEqual(plugs.slice(-3), [["lost"], ["error"], ["lost"]]);

  // The page goes back to watching: nothing while the cable is out, connected when it is in again.
  watch.start();
  assert.equal(await watch.looking, "nothing");
  server.plug();
  assert.equal(await watch.look(), "connected");
  assert.deepEqual(plugs.at(-1), ["connected", PORT, "plugged", 3]);
});

test("a server that does not say the robot was unplugged: the next scan says so", async () => {
  const server = mockServer({ moves: [], quietUnplug: true });
  const api = new RobotApi(server.fetch);
  await api.connect("multicuber", PORT);
  const { robot, heard, over } = follow(server, 3, () => null, { until: "error" });
  await robot.follow();
  server.unplug();
  await robot.scan();
  await over;
  robot.unfollow();
  assert.equal(heard.find((h) => h[0] === "error")[1], "not_connected");
  // What the page asks after any error: whether the server still holds the robot.
  assert.equal((await api.robots()).connected, null);
});

// ------------------------------------------------- as the server wants it

test("every command the page sends is JSON from this page, as the server demands", async () => {
  const n = 3;
  const { facelets, solution } = scrambled(n);
  const pictures = facePictures(n, facelets);
  // A browser adds the page's own Origin to a POST: the mock refuses any other, and anything that is not JSON.
  const server = mockServer({ moves: solution, origin: HOST });
  const sent = [];
  const api = new RobotApi((url, init) => {
    sent.push([init.method, url, init.headers?.["content-type"], init.body]);
    return server.fetch(url, init);
  });
  await api.connect("multicuber", PORT, 3);
  let holding = 0;
  const frames = (rotate, size) => {
    const [face, turn] = PHOTOS[holding++];
    return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, size);
  };
  const { robot, heard, over } = follow({ fetch: (url, init) => api.fetch(url, init) }, n, frames);
  await robot.follow();
  await robot.scan();
  await over;
  robot.unfollow();
  await api.stop();
  await api.disconnect();
  assert.ok(heard.some((h) => h[0] === "done"));
  const posts = sent.filter(([method]) => method === "POST");
  assert.deepEqual([...new Set(posts.map(([, url]) => url))].sort(), ["/api/robot/connect", "/api/robot/disconnect", "/api/robot/photo-taken", "/api/robot/scan", "/api/robot/solve", "/api/robot/stop"]);
  for (const [, url, type, body] of posts) {
    assert.equal(type, "application/json", url);
    assert.doesNotThrow(() => JSON.parse(body), url);
  }
  // What the server does with anything else.
  assert.equal((await server.fetch("/api/robot/scan", { method: "POST", body: "{}" })).status, 415);
  assert.equal((await server.fetch("/api/robot/scan", { method: "POST", body: "{}", headers: { "content-type": "application/json", origin: "http://elsewhere.example" } })).status, 403);
  await assert.rejects(new RobotApi((url, init) => server.fetch(url, { ...init, headers: {} })).stop(), (e) => e.code === "bad_request");
});

test("the photos are read by what each event says: any order, any turns", async () => {
  const n = 3;
  const { facelets, text } = scrambled(n);
  const pictures = facePictures(n, facelets);
  // The 2018 order with the server's turns, the server's order from the parked robot, and one nobody uses.
  for (const photos of [
    [["U", 90], ["R", 180], ["D", 270], ["L", 0], ["B", 0], ["F", 0]],
    [["L", 0], ["U", 90], ["R", 180], ["B", 180], ["D", 0], ["F", 0]],
    [["F", 270], ["D", 180], ["B", 90], ["U", 0], ["L", 270], ["R", 90]],
  ]) {
    const server = mockServer({ moves: [], photos });
    let holding = 0;
    const frames = (rotate, size) => {
      const [face, turn] = photos[holding++];
      return grab(cameraFrame(pictures["URFDLB".indexOf(face)], turn), rotate, size);
    };
    const { robot, heard, over } = follow(server, n, frames, { until: "idle" });
    await robot.follow();
    await robot.scan();
    await Promise.race([over, new Promise((resolve) => setTimeout(resolve, 5000))]);
    robot.unfollow();
    assert.deepEqual(heard.filter((h) => h[0] === "photo").map((h) => "URFDLB"[h[2]]), photos.map(([face]) => face));
    const [, scanned] = heard.find((h) => h[0] === "scanned");
    // Every picture upright in its own place: the cube as it is, with nothing put right.
    assert.equal(scanned.kind, "taken", photos.join(" "));
    assert.equal(scanned.state, text);
  }
});

test("a board that goes by its name and is no robot is asked once, then left alone and not complained of", async () => {
  const server = mockServer({ ports: [], identify: true });
  const { watch, heard, clock, count } = watcher(server);
  watch.start();
  await watch.looking;
  // Plugged in and not asked yet: the server says nothing of what it is.
  server.plug("/dev/cu.usbmodem7", { stranger: true });
  assert.equal(choosePort(await new RobotApi(server.fetch).robots()).sure, false);
  assert.equal(await watch.look(), "failed");
  assert.deepEqual(heard.at(-1), ["failed", "no_answer", false]);
  // From now on the server says it is none: never again, however long it stays.
  for (let k = 0; k < 10; k++) {
    clock.t += 60_000;
    assert.equal(await watch.look(), "nothing");
  }
  assert.equal(count("/api/robot/connect"), 1);
  // The robot beside it is connected all the same.
  server.plug(PORT);
  assert.equal(await watch.look(), "connected");
});

test("the robot picker: the robot held, and every other port with what the server knows of it", () => {
  const drivers = [{ id: "multicuber", name: "MultiCuber", ports: ["/dev/ttyUSB1", "/dev/ttyACM0", "/dev/ttyACM1"] }];
  const ports = [
    { path: "sim", driver: "multicuber", busy: false },
    { path: "/dev/ttyACM0", driver: null, busy: false },
    { path: "/dev/ttyACM1", busy: true },
    { path: "/dev/ttyUSB1", busy: false },
    { path: "/dev/cu.usbmodem21301", driver: "multicuber", busy: false },
  ];
  const choices = robotChoices({ drivers, ports }, { driver: "multicuber", port: "/dev/cu.usbmodem21301" });
  assert.deepEqual(choices.map((c) => [c.text, c.driver, c.current]), [
    ["MultiCuber · ttyUSB1 · checking", "multicuber", false],
    ["ttyACM0 · not a robot", null, false],
    ["ttyACM1 · busy", null, false],
    ["Simulated MultiCuber", "multicuber", false],
    ["MultiCuber · cu.usbmodem21301", "multicuber", true],
  ]);
  // The simulated robot held: named as that, with no state of its own.
  assert.equal(robotChoices({ drivers, ports }, { driver: "multicuber", port: "sim" }).find((c) => c.current).text, "Simulated MultiCuber");
});

test("the robot picker: a server that does not tell of its ports, and a robot held that it no longer lists", () => {
  const drivers = [{ id: "multicuber", name: "MultiCuber", ports: ["/dev/ttyUSB1"] }];
  // Nothing is being checked by a server that does not check: the port goes by its name.
  assert.deepEqual(robotChoices({ drivers }, null).map((c) => [c.text, c.driver, c.state]), [["MultiCuber · ttyUSB1", "multicuber", ""]]);
  // The robot held stays in the list, also when its port is not known.
  assert.deepEqual(robotChoices({ drivers }, { driver: "multicuber", port: "/dev/gone" }).map((c) => [c.text, c.current]), [["MultiCuber · ttyUSB1", false], ["MultiCuber · gone", true]]);
  assert.deepEqual(robotChoices({ drivers: [] }, { driver: "multicuber", port: "" }).map((c) => [c.path, c.text, c.current]), [["", "multicuber", true]]);
  // Claimed by two robots and neither the one of last time: not to be picked blindly.
  const two = [{ id: "a", name: "A", ports: ["/dev/x"] }, { id: "b", name: "B", ports: ["/dev/x"] }];
  assert.equal(robotChoices({ drivers: two }, null)[0].driver, null);
  assert.equal(robotChoices({ drivers: two }, null, { driver: "b", port: "/dev/x" })[0].text, "B · x");
});

test("the details of a scan: faces found, the sizes and times of the reading", () => {
  const rows = scanMeasures({
    sources: [{ found: true }, { found: true }, { found: false }, { found: true }, { found: true }, { found: true }],
    sizes: [480, 720, 720, 720, 720, 320],
    read_ms: [30.2, 61.7, 58.1, 60, 64.4, 14.6],
    work_ms: 138,
    grab_ms: 16.6,
    analysis_ms: 0,
  });
  assert.deepEqual(rows, [
    ["Faces found", "5 of 6"],
    ["Photos read at", "320 to 720 px (equivalent square)"],
    ["Reading a face", "15 to 64 ms"],
    ["Reading in all", "138 ms"],
    ["Slowest photo grab", "17 ms"],
    ["Ready after the scan", "0 ms"],
  ]);
  // A scan of which nothing was measured says what it can.
  assert.deepEqual(scanMeasures({ sources: [] }), [["Faces found", "0 of 6"]]);
});

test("the info button: a mouse over it shows the details, a click or a tap pins them, Escape and anything else closes them", () => {
  const button = new EventTarget(), area = new EventTarget(), doc = new EventTarget();
  const details = { hidden: true };
  let expanded = "false";
  button.parentElement = area;
  button.setAttribute = (name, value) => {
    assert.equal(name, "aria-expanded");
    expanded = value;
  };
  area.contains = (target) => target === button || target === details;
  const close = bindInfo(button, details, doc);
  const send = (target, type, props) => target.dispatchEvent(Object.assign(new Event(type), props));
  const open = () => {
    assert.equal(expanded, String(!details.hidden));
    return !details.hidden;
  };
  // A finger coming down is no hover: only its tap opens, and the next one closes.
  send(area, "pointerenter", { pointerType: "touch" });
  assert.ok(!open());
  send(button, "click");
  assert.ok(open());
  send(area, "pointerleave", { pointerType: "touch" });
  assert.ok(open());
  send(button, "click");
  assert.ok(!open());
  // A mouse: there while it is over the button or the details, gone when it leaves.
  send(area, "pointerenter", { pointerType: "mouse" });
  assert.ok(open());
  send(area, "pointerleave", { pointerType: "mouse" });
  assert.ok(!open());
  // Clicked while hovering it stays when the mouse leaves; a second click lets it go.
  send(area, "pointerenter", { pointerType: "mouse" });
  send(button, "click");
  send(area, "pointerleave", { pointerType: "mouse" });
  assert.ok(open());
  send(button, "click");
  assert.ok(!open());
  // The keyboard's Enter is a click. Escape, a click elsewhere and the focus going elsewhere close it.
  for (const [type, props] of [["keydown", { key: "Escape" }], ["pointerdown"], ["focusin"]]) {
    send(button, "click");
    assert.ok(open());
    send(doc, type, props);
    assert.ok(!open(), type);
  }
  // Not what happens inside it, nor another key.
  send(button, "click");
  send(doc, "keydown", { key: "a" });
  assert.ok(open());
  close();
  assert.ok(!open());
});

test("disconnecting during camera permission stops a late stream", async (t) => {
  let grant;
  const stream = new Promise(resolve => { grant = resolve; });
  const video = { srcObject: null };
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true, value: { getUserMedia: () => stream },
  });
  t.after(() => { delete navigator.mediaDevices; });
  const feed = new CameraFeed(video);
  const opened = feed.open();
  feed.close();
  let stopped = false;
  grant({ getTracks: () => [{ stop() { stopped = true; } }] });
  await assert.rejects(opened, { name: "AbortError" });
  assert.equal(video.srcObject, null);
  assert.ok(stopped);
});
