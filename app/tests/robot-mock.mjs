// A mock of the anycube server's robot API (robots-spec/API.md) as a
// fetch function: for the tests, and behind tests/robot-mock-server.mjs
// for the page in a browser.
//
// The robot as the server shows it: a scan takes six photos in robot.py's
// order and turns, waiting for photo-taken after each; a solve plans, then
// runs its ops one after another. `script` changes what it does:
//
//   moves          the solution the planner finds (names, as anycube writes them)
//   stepMs         time between steps (2: the tests are quick)
//   settleMs       from turning a face to the camera to its `photo` event
//   opMs(op)       how long an op takes (stepMs unless given)
//   computeMs      how long the planner takes
//   maxWaitMs      the longest an events request is held
//   failAfterPhoto an error instead of the photo after this one
//   down(path)     true: that request fails as with no server

export const json = (status, body) => ({ ok: status < 400, status, json: async () => body });

export const PHOTOS = [["U", 270], ["R", 180], ["D", 90], ["L", 0], ["B", 0], ["F", 0]];

export const DRIVERS = [{ id: "multicuber", name: "MultiCuber", sizes: [2, 3, 4, 5], ports: ["/dev/cu.usbmodem21301"] }];

export function mockServer(script = {}) {
  const events = [];
  const calls = [];
  let seq = 0;
  let runs = 0;
  let run = null;
  let began = 0;
  let times = {};
  let stopped = false;
  let waiting = [];
  let connected = script.connected ?? null;
  const server = { events, calls, holding: -1 };
  const push = (type, body = {}) => {
    events.push({ seq: ++seq, type, ...(run && type !== "log" ? { run, t_ms: Math.round(Date.now() - began) } : {}), ...body });
    for (const wake of waiting.splice(0)) wake();
  };
  const after = (ms, fn) => setTimeout(() => !stopped && fn(), ms);
  const step = (fn) => after(script.stepMs ?? 2, fn);
  // The robot turns a face to the camera, waits for it to settle, asks for the photo.
  const photo = (index) => {
    server.holding = index;
    after(script.settleMs ?? 0, () => push("photo", { index, face: PHOTOS[index][0], rotate: PHOTOS[index][1] }));
  };
  server.push = push;
  server.fetch = async (url, init = {}) => {
    const method = init.method ?? "GET";
    const body = init.body ? JSON.parse(init.body) : undefined;
    const u = new URL(url, "http://127.0.0.1");
    calls.push({ method, path: u.pathname, body });
    if (script.down?.(u.pathname)) throw new TypeError("Failed to fetch");
    switch (u.pathname) {
      case "/api/robots":
        return json(200, { drivers: DRIVERS, connected });
      case "/api/robot/connect":
        connected = { driver: body.driver, port: body.port ?? DRIVERS[0].ports[0], size: script.size ?? 3, info: { firmware: "2.2" } };
        return json(200, { ok: true, size: connected.size, info: connected.info, port: connected.port });
      case "/api/robot/disconnect":
        connected = null;
        return json(200, { ok: true });
      case "/api/robot/events": {
        const since = Number(u.searchParams.get("since"));
        const wait = Number(u.searchParams.get("wait"));
        if (!events.some((e) => e.seq > since) && wait > 0) {
          await new Promise((resolve, reject) => {
            waiting.push(resolve);
            setTimeout(resolve, Math.min(wait * 1000, script.maxWaitMs ?? 30));
            init.signal?.addEventListener("abort", () => reject(Object.assign(new Error("aborted"), { name: "AbortError" })));
          });
        }
        return json(200, { seq, events: events.filter((e) => e.seq > since) });
      }
      case "/api/robot/scan": {
        run = `run-${++runs}`;
        began = Date.now();
        stopped = false;
        times = {};
        step(() => {
          push("scan-start", { steps: 22, eta_ms: 9000 });
          photo(0);
        });
        return json(200, { ok: true, run });
      }
      case "/api/robot/photo-taken": {
        if (body.run !== run) return json(409, { ok: false, error: { code: "stale_run", message: "Not the current run." } });
        const next = body.index + 1;
        server.holding = -1; // turning to the next face
        if (script.failAfterPhoto === body.index) {
          step(() => push("error", { code: "jam", message: "The lift did not reach its position." }));
        } else if (next < 6) {
          step(() => photo(next));
        } else {
          step(() => {
            times.scan_ms = Date.now() - began;
            push("scan-done", { scan_ms: times.scan_ms });
          });
        }
        return json(200, { ok: true });
      }
      case "/api/robot/solve": {
        if (body.run !== run) return json(409, { ok: false, error: { code: "stale_run", message: "Not the current run." } });
        const moves = script.moves;
        const ops = moves.flatMap((m, k) => [
          { cmd: "LIFT", arg: "1", est_ms: 425, move: null },
          { cmd: "ROT", arg: m.endsWith("2") ? "2" : "1", est_ms: m.endsWith("2") ? 672 : 569, move: k },
        ]);
        const solving = Date.now();
        after(script.computeMs ?? script.stepMs ?? 2, () => {
          times.compute_ms = Date.now() - solving;
          push("plan", { moves, ops, eta_ms: ops.reduce((a, o) => a + o.est_ms, 0), compute_ms: times.compute_ms, candidates: 6, metric: "robot-ms" });
          const executing = Date.now();
          let i = 0;
          const next = () => {
            if (i === ops.length) {
              push("done", { times: { scan_ms: times.scan_ms, analysis_ms: body.analysis_ms, compute_ms: times.compute_ms, exec_ms: Date.now() - executing, total_ms: Date.now() - began } });
              return;
            }
            const op = ops[i];
            const ms = script.opMs?.(op) ?? script.stepMs ?? 2;
            if (op.move !== null) push("move", { move: op.move, est_ms: op.est_ms });
            after(ms, () => {
              push("op", { i, cmd: op.cmd, arg: op.arg, actual_ms: script.opMs ? ms : op.est_ms * 1.1 });
              i++;
              next();
            });
          };
          next();
        });
        return json(200, { ok: true });
      }
      case "/api/robot/stop":
        stopped = true;
        setTimeout(() => push("error", { code: "stopped", message: "Stopped: the motors are off." }), 0);
        return json(200, { ok: true });
      default:
        return json(404, { ok: false, error: { code: "not_found", message: "no such endpoint" } });
    }
  };
  return server;
}
