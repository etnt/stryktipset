#!/usr/bin/env node
// I18n test for reduce.html / reduce-rs/template.html:
//   1. the `sv` and `en` dictionaries have the same keys,
//   2. every data-i18n attribute in the template has a dictionary key,
//   3. translateError() maps the known Rust error strings to Swedish.
//
// Run from reduce-rs:
//   node tests/i18n.mjs
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const template = fs.readFileSync(path.join(root, "template.html"), "utf8");

const script = template.match(/<script>([\s\S]*)<\/script>/)[1];

// Minimal DOM/localStorage stubs so the script's i18n block can run.
const sandbox = {
  document: {
    documentElement: { lang: "" },
    title: "",
    querySelectorAll: () => [],
    getElementById: () => ({ textContent: "", classList: { toggle() {}, add() {}, remove() {} }, setAttribute() {}, onclick: null }),
  },
  localStorage: { getItem: () => null, setItem() {} },
  window: {},
};
vm.createContext(sandbox);

// Evaluate only the i18n section: from `const I18N` up to `const langSvBtn`,
// plus the small helpers it needs.
const start = script.indexOf("const I18N");
const end = script.indexOf("const langSvBtn");
if (start < 0 || end < 0) {
  console.error("error: could not locate the I18N block in template.html");
  process.exit(1);
}
// Slice up to the lastStatus declaration if it is inside the range.
const lastStatusIdx = script.slice(start, end).indexOf("let lastStatus");
const stop = lastStatusIdx >= 0 ? start + lastStatusIdx : end;
vm.runInContext(
  script.slice(start, stop) +
    "\nvar lastStatus = null;" +
    "\nfunction setStatus(key, ...args) { lastStatus = { key, args }; }" +
    "\nvar statusText = { textContent: '' };" +
    "\nglobalThis.I18N = I18N;" +
    "\nglobalThis.setLang = (l) => { lang = l; };" +
    "\nglobalThis.translateError = translateError;",
  sandbox,
);

let failures = 0;
const fail = (msg) => { console.error("FAIL: " + msg); failures++; };

// --- 1. sv and en have the same keys ---------------------------------------
const { I18N } = sandbox;
const setLang = sandbox.setLang;
const translateError = sandbox.translateError;
const svKeys = Object.keys(I18N.sv).sort();
const enKeys = Object.keys(I18N.en).sort();
const missingEn = svKeys.filter((k) => !(k in I18N.en));
const missingSv = enKeys.filter((k) => !(k in I18N.sv));
if (missingEn.length) fail("keys missing in en: " + missingEn.join(", "));
if (missingSv.length) fail("keys missing in sv: " + missingSv.join(", "));
console.log(`ok: ${svKeys.length} i18n keys, sv/en key sets ${missingEn.length + missingSv.length === 0 ? "match" : "differ"}`);

// --- 2. every data-i18n* attribute exists in the dictionary ----------------
const attrKeys = new Set();
for (const m of template.matchAll(/data-i18n(?:-html|-title|-placeholder)?="([^"]+)"/g)) {
  attrKeys.add(m[1]);
}
const unknown = [...attrKeys].filter((k) => !(k in I18N.sv));
if (unknown.length) fail("data-i18n attributes without a dictionary key: " + unknown.join(", "));
else console.log(`ok: all ${attrKeys.size} data-i18n attribute keys exist in the dictionary`);

// --- 3. translateError maps the known Rust errors --------------------------
setLang("sv");
const cases = [
  ["the system must have 13 entries, got 12: '1,x'", "systemet måste ha 13 tecken, fick 12: '1,x'"],
  ["match 3: empty entry", "match 3: tom rad"],
  ["match 4: invalid sign(s) 3 in '1,3'; use 1, X and 2", "match 4: ogiltigt tecken 3 i '1,3'; använd 1, X och 2"],
  ["match 5: repeated sign in '11'", "match 5: upprepat tecken i '11'"],
  ["--guarantee must be 1..13", "--guarantee måste vara 1..13"],
  ["--rows/--max-rows must be at least 1", "--rows/--max-rows måste vara minst 1"],
  ["--rows 200 is more than the 100 rows of the full system", "--rows 200 är fler än de 100 raderna i hela systemet"],
  [
    "neighbour table too large (100 x 50 cells). Use a smaller system, a higher --guarantee, or raise --max-memory-cells.",
    "grannatabellen är för stor (100 x 50 celler). Använd ett mindre system, en högre --guarantee, eller höj --max-memory-cells.",
  ],
  ["1000 rows exceeds the file limit of 5000 single rows", "1000 rader överskrider filgränsen på 5000 enkelrader"],
  ["cost 6000 SEK exceeds the limit of 3000 SEK", "kostnaden 6000 kr överskrider gränsen på 3000 kr"],
];
for (const [raw, expected] of cases) {
  const got = translateError(raw);
  if (got !== expected) fail(`translateError(${JSON.stringify(raw)})\n  got:      ${got}\n  expected: ${expected}`);
}
console.log(`ok: ${cases.length} error translations match`);

// Unknown errors must pass through unchanged in both languages.
const passthrough = "some unknown engine error";
for (const l of ["sv", "en"]) {
  setLang(l);
  const got = translateError(passthrough);
  if (got !== passthrough) fail(`translateError passthrough in ${l}: got ${got}`);
}
console.log("ok: unknown errors pass through unchanged");

// --- 4. DOM-level smoke test: run the whole page script with a stubbed DOM --
// The initial applyLang() and every lang toggle must run without throwing
// and update the status, estimate placeholder and static texts. jsdom is
// not a dependency, so a minimal permissive DOM stub is used instead.
try {
  // Element stubs discovered from the static HTML.
  const i18nEls = { i18n: new Map(), html: new Map(), title: new Map(), placeholder: new Map() };
  const dsProp = { i18n: "i18n", html: "i18nHtml", title: "i18nTitle", placeholder: "i18nPlaceholder" };
  for (const m of template.matchAll(/data-i18n(-html|-title|-placeholder)?="([^"]+)"/g)) {
    const kind = m[1] ? m[1].slice(1) : "i18n";
    if (!i18nEls[kind].has(m[2])) {
      const el = makeStubEl("i18n:" + m[2]);
      el.dataset[dsProp[kind]] = m[2]; // what applyLang() reads back
      i18nEls[kind].set(m[2], el);
    }
  }
  const htmlIds = [...template.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]);

  function makeStubEl(id) {
    const classes = new Set();
    const attrs = {};
    const el = {
      id, textContent: "", innerHTML: "", value: "", checked: false,
      disabled: false, title: "", placeholder: "", dataset: {}, attrs,
      classList: {
        toggle(c, on) { classes[on ? "add" : "delete"](c); return on; },
        add: (c) => classes.add(c),
        remove: (c) => classes.delete(c),
        contains: (c) => classes.has(c),
      },
      setAttribute: (k, v) => { attrs[k] = String(v); },
      getAttribute: (k) => (attrs[k] === undefined ? "" : attrs[k]),
      addEventListener() {}, removeEventListener() {},
      querySelector() { return makeStubEl(id + "?q"); },
      querySelectorAll: () => [],
      appendChild() {}, closest() { return null; }, click() {},
    };
    return el;
  }

  const byId = new Map();
  const getById = (id) => {
    if (!byId.has(id)) byId.set(id, makeStubEl(id));
    return byId.get(id);
  };
  for (const id of htmlIds) getById(id); // pre-create every id in the HTML

  const doc = {
    addEventListener() {},
    documentElement: makeStubEl("html"),
    title: "",
    getElementById: getById,
    createElement: (tag) => makeStubEl("created:" + tag),
    querySelectorAll: (sel) => {
      const m = sel.match(/^\[data-i18n(-html|-title|-placeholder)?\]$/);
      return m ? [...i18nEls[m[1] ? m[1].slice(1) : "i18n"].values()] : [];
    },
  };

  const dom = {
    document: doc,
    localStorage: { store: {}, getItem(k) { return this.store[k] ?? null; }, setItem(k, v) { this.store[k] = String(v); } },
    URL: { createObjectURL: () => "blob:stub", revokeObjectURL() {} },
    Blob: class Blob { constructor() {} },
    Worker: class Worker { postMessage() {} terminate() {} addEventListener() {} removeEventListener() {} },
    navigator: {},
    setTimeout, clearTimeout, setInterval, clearInterval,
  };
  // The template carries build placeholders (inline.py injects the wasm
  // glue, module and version). Stub them with harmless values: the glue is
  // only ever string-concatenated into the worker source, which the Worker
  // stub never executes.
  const page = script
    .replace("__GLUE_JSON__", '"// stub wasm glue"')
    .replace("__WASM_B64__", "")
    .replace("__VERSION__", "test");

  vm.createContext(dom);
  vm.runInContext(page, dom, { filename: "template.html" });

  const $id = (id) => getById(id);
  const assertEq = (got, expected, what) => {
    if (got !== expected) fail(`smoke: ${what}\n  got:      ${JSON.stringify(got)}\n  expected: ${JSON.stringify(expected)}`);
  };

  // Initial state: Swedish default, placeholder filled, status Ready/Klar.
  assertEq(doc.documentElement.lang, "sv", "initial documentElement.lang");
  assertEq($id("statusText").textContent, "Klar.", "initial status text (sv)");
  assertEq($id("estimate").textContent, "Ange ett system för att se hela systemets storlek.", "initial estimate placeholder (sv)");
  assertEq(i18nEls.i18n.get("run").textContent, "Reducera systemet", "initial static run label (sv)");
  if (!$id("langSv").classList.contains("on")) fail("smoke: langSv button is not marked active after init");

  // Toggle to English.
  $id("langEn").onclick();
  assertEq(doc.documentElement.lang, "en", "documentElement.lang after en toggle");
  assertEq($id("statusText").textContent, "Ready.", "status text after en toggle");
  assertEq($id("estimate").textContent, "Enter a system to see the full-system size.", "estimate placeholder after en toggle");
  assertEq(i18nEls.i18n.get("run").textContent, "Reduce system", "static run label after en toggle");
  assertEq($id("langEn").attrs["aria-pressed"], "true", "langEn aria-pressed after en toggle");
  assertEq($id("langSv").attrs["aria-pressed"], "false", "langSv aria-pressed after en toggle");
  if (!$id("langEn").classList.contains("on")) fail("smoke: langEn button is not marked active after en toggle");
  if ($id("langSv").classList.contains("on")) fail("smoke: langSv button still marked active after en toggle");

  // Toggle back to Swedish.
  $id("langSv").onclick();
  assertEq(doc.documentElement.lang, "sv", "documentElement.lang after sv toggle back");
  assertEq($id("statusText").textContent, "Klar.", "status text after sv toggle back");
  assertEq($id("estimate").textContent, "Ange ett system för att se hela systemets storlek.", "estimate placeholder after sv toggle back");

  console.log("ok: DOM smoke test — full script ran and toggled sv/en without exceptions");
} catch (e) {
  fail("smoke: page script threw: " + (e && e.stack ? e.stack : e));
}

if (failures) process.exit(1);
console.log("i18n test passed");
