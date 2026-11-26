// The page with a robot that is not there: serves app/ as `anycube app`
// does, with a server that says it drives robots and a mock robot behind
// it (robot-mock.mjs). For trying the robot extension in a browser:
//
//   node app/tests/robot-mock-server.mjs [port] [--size N] [--moves "R U F' ..."] [--fast] [--unplugged] [--names-only]
//
// The robot's cable is at /mock/plug and /mock/unplug (--unplugged starts
// with it out); --names-only is a server that does not say what its ports
// are, only their names. /mock/log?text=... is a `log` event of the robot,
// /mock/asked how often /api/robots was asked.
//
// The camera the robot would hold up is at /mock/frame/<photo>.png (what
// the camera sees of each face: the face turned back by the photo's
// rotate, in the middle of a 16:9 picture) and /mock/holding says which
// photo the robot is holding up now (-1 between photos). A browser's own
// camera sees no robot, so a page that is to read these pictures needs
// its getUserMedia replaced by a canvas that draws them (as the screenshot
// script does); with a real camera the scan reads whatever it sees.
//
// With MultiCuber/CubeScan beside this repository and --size 4, the
// robot's own pictures of a 4x4x4 are used; otherwise pictures of a cube
// scrambled with --moves are drawn.

import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { CubeModel } from "../js/cube-model.js";
import { rotateSquare } from "../js/robot.js";
import { picture, quad } from "./pictures.mjs";
import { readPng, writePng } from "./png.mjs";
import { HOST, mockServer, PHOTOS } from "./robot-mock.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const port = Number(args.find((a) => /^\d+$/.test(a)) ?? 8765);
const size = Number(option("--size", 3));
const fast = args.includes("--fast");
const APP = join(import.meta.dirname, "..");
const CUBESCAN = join(APP, "../../MultiCuber/CubeScan");

// The six faces as the robot's photos keep them (URFDLB), and the moves that solve the cube.
function faces() {
  const model = new CubeModel(size);
  if (size === 4) {
    try {
      const pics = [..."URFDLB"].map((f) => {
        const p = readPng(join(CUBESCAN, `rubiks-side-${f}.png`));
        return "UD".includes(f) ? { ...p, data: rotateSquare(p.data, p.width, 180) } : p;
      });
      return { pics, solution: ["R'"] };
    } catch {
      // No CubeScan: drawn pictures.
    }
  }
  const scramble = model.parse(option("--moves", "R U F' L2 D B' R2 U' F2 L D' B2 R' U2"));
  const facelets = model.applyAll(model.solved(), scramble);
  const nn = size * size;
  const side = 480;
  const pics = [0, 1, 2, 3, 4, 5].map((f) =>
    picture({ W: side, H: side, n: size, corners: quad(side / 2, side / 2, side * 0.74, 1, 0.01, 0.01), stickers: Array.from(facelets.slice(f * nn, (f + 1) * nn)), background: [52, 46, 40], noise: 3, seed: f + 1 }),
  );
  return { pics, solution: model.names(scramble.map((m) => model.inverse(m)).reverse()).split(" ") };
}

const { pics, solution } = faces();
// What the camera sees at each photo: the face turned back, in a 16:9 picture.
const frames = PHOTOS.map(([face, rotate]) => {
  const p = pics["URFDLB".indexOf(face)];
  const side = p.width;
  const turned = rotateSquare(p.data, side, (360 - rotate) % 360);
  const W = Math.round((side * 16) / 9);
  const data = new Uint8ClampedArray(W * side * 4);
  for (let i = 0; i < W * side; i++) data.set([58, 60, 66, 255], i * 4);
  const x0 = Math.floor((W - side) / 2);
  for (let y = 0; y < side; y++) data.set(turned.subarray(y * side * 4, (y + 1) * side * 4), (y * W + x0) * 4);
  return writePng({ width: W, height: side, data });
});

const robot = mockServer({
  moves: solution,
  size,
  stepMs: fast ? 60 : 700,
  settleMs: fast ? 400 : 250,
  computeMs: fast ? 40 : 180,
  opMs: (op) => Math.round(op.est_ms * (fast ? 0.25 : 1.04)),
  maxWaitMs: 10_000,
  identify: !args.includes("--names-only"),
  ports: args.includes("--unplugged") ? [] : undefined,
});

const TYPES = { ".html": "text/html; charset=utf-8", ".css": "text/css; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".svg": "image/svg+xml", ".png": "image/png", ".wasm": "application/wasm" };
const CONFIG = { max_size: 5, browser_sizes: [2, 3], modes: ["fast", "normal", "shorter"], queue: 0, time_cap_s: 0, max_scramble_moves: 5000, repo_url: null, local: true, top_colour: "white", scan_orientation: "capture", version: "0.0.0-mock", robots: true };

const send = (res, status, type, body) => {
  res.writeHead(status, { "content-type": type, "cache-control": "no-store" });
  res.end(body);
};

createServer(async (req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${port}`);
  const path = url.pathname;
  let body = "";
  for await (const part of req) body += part;
  if (path === "/api/config") return send(res, 200, "application/json", JSON.stringify(CONFIG));
  if (path === "/api/check") return send(res, 200, "application/json", JSON.stringify({ ok: true }));
  if (path === "/api/scramble") return send(res, 200, "application/json", JSON.stringify({ scramble: "R U R' U'" }));
  if (path === "/mock/plug" || path === "/mock/unplug") {
    robot[path.slice(6)]();
    return send(res, 200, "application/json", JSON.stringify({ ok: true }));
  }
  if (path === "/mock/log") {
    robot.push("log", { text: url.searchParams.get("text") ?? "" });
    return send(res, 200, "application/json", JSON.stringify({ ok: true }));
  }
  if (path === "/mock/asked") return send(res, 200, "application/json", JSON.stringify({ robots: robot.calls.filter((c) => c.path === "/api/robots").length }));
  if (path === "/mock/holding") return send(res, 200, "application/json", JSON.stringify({ index: robot.holding }));
  const frame = /^\/mock\/frame\/(\d)\.png$/.exec(path);
  if (frame) return send(res, 200, "image/png", frames[Number(frame[1])]);
  if (path.startsWith("/api/robot")) {
    // The headers the server looks at go with the request: a command that is not JSON from this page is refused here too.
    const headers = { ...(req.headers["content-type"] ? { "content-type": req.headers["content-type"] } : {}), ...(req.headers.origin ? { origin: req.headers.origin === `http://${req.headers.host}` ? HOST : req.headers.origin } : {}) };
    const answer = await robot.fetch(req.url, { method: req.method, body: body || undefined, headers });
    return send(res, answer.status, "application/json", JSON.stringify(await answer.json()));
  }
  const file = normalize(join(APP, path === "/" ? "index.html" : path));
  if (!file.startsWith(APP) || file.includes("/tests/")) return send(res, 404, "text/plain", "not found");
  try {
    send(res, 200, TYPES[extname(file)] ?? "application/octet-stream", readFileSync(file));
  } catch {
    send(res, 404, "text/plain", "not found");
  }
}).listen(port, "127.0.0.1", () => console.log(`http://127.0.0.1:${port}/  (a mock ${size}x${size}x${size} robot; solution: ${solution.join(" ")})`));
