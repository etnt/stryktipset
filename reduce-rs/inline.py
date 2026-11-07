#!/usr/bin/env python3
"""Inline the wasm-bindgen output into the final self-contained HTML page.

Reads build/reduce_web.js (glue), build/reduce_web_bg.wasm and
template.html, then writes ../reduce.html with the glue embedded as a JSON
string literal and the wasm embedded as base64.
"""

import base64
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parent
build = root / "build"

glue = (build / "reduce_web.js").read_text(encoding="utf-8")
wasm_b64 = base64.b64encode((build / "reduce_web_bg.wasm").read_bytes()).decode("ascii")

# A literal "</script" inside the inlined JS would terminate the script tag.
if "</script" in glue.lower():
    sys.exit("error: glue JS contains '</script'; escaping logic needed")

template = (root / "template.html").read_text(encoding="utf-8")
if "__GLUE_JSON__" not in template or "__WASM_B64__" not in template:
    sys.exit("error: template.html is missing placeholders")

html = template.replace("__GLUE_JSON__", json.dumps(glue))
html = html.replace("__WASM_B64__", wasm_b64)

out = root.parent / "reduce.html"
out.write_text(html, encoding="utf-8")
print(f"wrote {out} ({out.stat().st_size / 1024:.0f} KB)")
