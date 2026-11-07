#!/usr/bin/env node
// Parity test: the wasm module (as used by reduce.html) must produce
// byte-identical system files as the native CLI for the same seed/settings.
//
// Run from tools/stryktipset/reduce-rs after ./build.sh:
//   node tests/parity.mjs
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const buildDir = path.join(root, "build");
const cliBin = path.join(root, "target", "release", "reduce-rs");

if (!fs.existsSync(path.join(buildDir, "reduce_web_bg.wasm"))) {
  console.error("error: run ./build.sh first (missing build/reduce_web_bg.wasm)");
  process.exit(1);
}

// Load the wasm-bindgen glue and init with the raw wasm bytes.
const glue = fs.readFileSync(path.join(buildDir, "reduce_web.js"), "utf8");
const wasmBytes = fs.readFileSync(path.join(buildDir, "reduce_web_bg.wasm"));
const wasm_bindgen = new Function(glue + "\n;return wasm_bindgen;")();
await wasm_bindgen(wasmBytes);

const json = (fn, ...args) => JSON.parse(fn(...args));

// The same case matrix as tests/golden.sh. `time_limit` is intentionally
// excluded: the search stops on wall-clock time, so wasm and native runs
// would stop at different points even with the same seed. Time-limited runs
// are therefore untested in parity; tests/golden.sh only checks that the flag
// is parsed, with a limit high enough never to trigger.
const cases = [
  { name: "greedy12", args: ["-g12", "-r3", "--seed", "42"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { restarts: 3, seed: 42 } },
  { name: "greedy13", args: ["-g13", "--seed", "42"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { guarantee: 13, seed: 42 } },
  { name: "maxrows", args: ["-g12", "--max-rows", "4", "--seed", "7"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { max_rows: 4, seed: 7 } },
  { name: "rows10", args: ["-g12", "--rows", "10", "--seed", "7"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { rows: 10, seed: 7 } },
  { name: "single", args: ["-g12", "--seed", "5"], sys: "1,1,1,1,1,1,1,1,1,1,1,1,1", p: { seed: 5 } },
  {
    name: "crlf", args: ["-g12", "-r3", "--seed", "42", "--crlf", "--product", "Europatipset"],
    sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { restarts: 3, seed: 42, crlf: true, product: "Europatipset" },
  },
  { name: "anneal", args: ["-g12", "-a", "anneal", "--seed", "3", "-r", "2", "-i", "20000"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { algorithm: "anneal", seed: 3, restarts: 2, iterations: 20000 } },
  { name: "noverify", args: ["-g12", "-r3", "--seed", "42", "--no-verify"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { restarts: 3, seed: 42 } },
  { name: "hitdist", args: ["-g12", "-r3", "--seed", "42"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { restarts: 3, seed: 42, hit: true }, checkHit: true },
  { name: "quiet", args: ["-g12", "-r3", "--seed", "42", "-q"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { restarts: 3, seed: 42 }, checkQuiet: true },
  { name: "seedbig", args: ["-g12", "-r3", "--seed", "3000000000"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { restarts: 3, seed: "3000000000" }, checkSeedEcho: "3000000000" },
  // Error paths: the CLI message (after "error: ") must equal the wasm error.
  { name: "err_system", args: ["-g12"], sys: "1,2", p: {}, expectErr: true },
  { name: "err_guarantee", args: ["-g0"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { guarantee: 0 }, expectErr: true },
  { name: "err_rows", args: ["-k", "100"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { rows: 100 }, expectErr: true },
  { name: "err_rowsneg", args: ["-k", "0"], sys: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", p: { rows: 0 }, expectErr: true },
  { name: "err_toolarge", args: ["-g1"], sys: "1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2", p: { guarantee: 1 }, expectErr: true },
];

let failures = 0;
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "reduce-parity-"));

for (const c of cases) {
  // --- CLI reference run -------------------------------------------------
  const dir = path.join(tmp, c.name);
  fs.mkdirSync(dir);
  let cliErr = null, cliFileText = null, cliStdout = null;
  try {
    cliStdout = execFileSync(cliBin, [...c.args, c.sys, "-o", path.join(dir, "out.txt")], { cwd: dir, stdio: "pipe" }).toString();
    cliFileText = fs.readFileSync(path.join(dir, "out.txt"), "utf8");
  } catch (e) {
    cliErr = e.stderr.toString().replace(/^error: /, "").trim();
  }

  // --- wasm run ----------------------------------------------------------
  const params = { system: c.sys, product: c.p.product || "Stryktipset", crlf: !!c.p.crlf, ...c.p };
  const res = json(wasm_bindgen.reduce, JSON.stringify(params));

  if (c.expectErr) {
    if (res.ok) { console.log(`FAIL ${c.name}: expected error, got ok`); failures++; continue; }
    if (res.error !== cliErr) {
      console.log(`FAIL ${c.name}: error text mismatch\n  cli : ${cliErr}\n  wasm: ${res.error}`);
      failures++;
    } else {
      console.log(`ok   ${c.name} (error parity)`);
    }
    continue;
  }

  if (!res.ok) { console.log(`FAIL ${c.name}: wasm error: ${res.error}`); failures++; continue; }
  if (res.text !== cliFileText) {
    console.log(`FAIL ${c.name}: file bytes differ from CLI output`);
    failures++;
    continue;
  }
  if (c.checkHit) {
    // Parse the CLI's "N correct: M outcomes" lines into a map and compare
    // it with the wasm hit distribution, sorted k=v pair by pair.
    const dist = {};
    for (const m of cliStdout.matchAll(/^\s+(\d+) correct:\s+(\d+) outcomes/gm)) dist[m[1]] = Number(m[2]);
    const cli = Object.entries(dist).map(([k, v]) => k + "=" + v).sort().join(",");
    const wasm = Object.entries(res.hit_dist || {}).map(([k, v]) => k + "=" + v).sort().join(",");
    if (!Object.keys(dist).length || cli !== wasm) {
      console.log(`FAIL ${c.name}: hit distribution differs\n  cli : ${cli || "(none)"}\n  wasm: ${wasm || "(none)"}`);
      failures++;
      continue;
    }
  }
  if (c.checkQuiet && cliStdout !== "") {
    console.log(`FAIL ${c.name}: CLI printed output with -q: ${JSON.stringify(cliStdout.slice(0, 120))}`);
    failures++;
    continue;
  }
  if (c.checkSeedEcho != null && res.seed !== c.checkSeedEcho) {
    console.log(`FAIL ${c.name}: wasm echoed seed ${JSON.stringify(res.seed)} instead of ${JSON.stringify(c.checkSeedEcho)}`);
    failures++;
    continue;
  }
  console.log(`ok   ${c.name} (${res.rows_written} rows, byte-identical to CLI)`);
}

fs.rmSync(tmp, { recursive: true, force: true });

// ---------------------------------------------------------------------------
// Hamming guarantee check: independently verify the coverage property by
// brute force over the full system.
// ---------------------------------------------------------------------------
function hammingCheck(sys, guarantee, rows) {
  const SIGNS = ["1", "X", "2"];
  const entries = sys.split(",").map((s) => {
    const set = new Set(s.toUpperCase().split("").map((c) => SIGNS.indexOf(c)));
    return SIGNS.filter((_, i) => set.has(i));
  });
  const size = entries.reduce((a, e) => a * e.length, 1);
  const radix = entries.map((e) => e.length);
  const divmod = [];
  let rem = 0;
  // Match 1 is the most significant digit, like the Rust model:
  // strip least-significant (match 13) first, filling d[m] = digit of match m+1.
  const digitsOf = (i) => {
    const d = new Array(13);
    for (let m = 12; m >= 0; m--) { d[m] = i % radix[m]; i = Math.floor(i / radix[m]); }
    return d;
  };
  const chosen = rows.map((line) => line.split(",").map((s, m) => entries[m].indexOf(s)));
  let minBest = 14;
  const covered = new Array(13).fill(0);
  for (let i = 0; i < size; i++) {
    const d = digitsOf(i);
    let best = 0;
    for (const row of chosen) {
      let hits = 0;
      for (let m = 0; m < 13; m++) if (row[m] === d[m]) hits++;
      if (hits > best) best = hits;
    }
    if (best >= guarantee) covered[best - guarantee]++; // count outcomes meeting guarantee
    minBest = Math.min(minBest, best);
  }
  return { size, minBest };
}

// Independent verification on the greedy12 result: every full-system outcome
// must be within distance 13-12 = 1 of some chosen row (i.e. best >= 12).
{
  const res = json(wasm_bindgen.reduce, JSON.stringify({
    system: "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", guarantee: 12, restarts: 3, seed: 42,
  }));
  if (!res.ok) { console.log("FAIL hamming: reduce failed"); failures++; }
  else {
    const rows = res.text.split("\n").slice(1).filter(Boolean).map((l) => l.replace(/^E,/, ""));
    const { size, minBest } = hammingCheck("1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x", 12, rows);
    if (minBest < 12) { console.log(`FAIL hamming: guarantee violated, min best = ${minBest}`); failures++; }
    else console.log(`ok   hamming guarantee 12 verified by brute force over ${size} outcomes`);
    // Cross-check coverage counts against the wasm result.
    if (res.covered !== size) { console.log(`FAIL hamming: covered ${res.covered} != size ${size}`); failures++; }
    else console.log("ok   coverage count matches wasm report");
  }
}

console.log(failures === 0 ? "\nALL PARITY CHECKS PASSED" : `\n${failures} PARITY CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
