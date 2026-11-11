// Solves cubes with the WebAssembly build, for the Rust test that compares
// it with the native solver (tests/wasm_native.rs).
//
//   node wasm-solve.mjs <module.wasm>
//
// Lines in:  size <TAB> moves|facelets <TAB> fast|normal <TAB> the cube
// Lines out: ok <TAB> the solution      or      err <TAB> code <TAB> message

import { readFileSync } from "node:fs";
import { WasmSolver } from "../js/wasm-solver.js";

const solver = await WasmSolver.create(readFileSync(process.argv[2]));
const lines = readFileSync(0, "utf8").split("\n").filter(Boolean);
for (const line of lines) {
  const [size, kind, mode, input] = line.split("\t");
  const r = solver.solve(Number(size), kind, mode, input);
  console.log(r.ok ? `ok\t${r.solution}` : `err\t${r.code}\t${r.message}`);
}
