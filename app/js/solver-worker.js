// The 2x2x2 and 3x3x3 solvers, off the page's thread: building their
// tables takes most of a second, and the page should not freeze for it.

import { WasmSolver } from "./wasm-solver.js";

let solver = null;

async function load() {
  if (!solver) {
    const response = await fetch(new URL("../pkg/anycube.wasm", import.meta.url));
    if (!response.ok) throw new Error(`the solver module answered ${response.status}`);
    solver = await WasmSolver.create(await response.arrayBuffer());
  }
  return solver;
}

self.onmessage = async ({ data }) => {
  const { id, op, size, kind, input, mode } = data;
  try {
    const s = await load();
    let result = { ok: true };
    if (op === "prepare") {
      s.prepare(size);
    } else if (op === "scramble") {
      result = { ok: true, scramble: s.randomState(size, data.seed) };
    } else if (op === "check") {
      result = s.check(size, kind, input);
    } else if (op === "solve") {
      const t = performance.now();
      result = s.solve(size, kind, mode, input);
      result.ms = performance.now() - t;
    }
    self.postMessage({ id, result });
  } catch (error) {
    // After a trap the module's memory cannot be trusted: start afresh next time.
    solver = null;
    self.postMessage({ id, crashed: String(error?.message || error) });
  }
};
