#!/usr/bin/env python3
"""Inline the wasm-bindgen output into the final self-contained HTML page.

Reads build/reduce_web.js (glue), build/reduce_web_bg.wasm and
template.html, then writes ../reduce.html with the glue embedded as a JSON
string literal and the wasm embedded as base64.
"""

import base64
import html
import json
import os
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
if "__GLUE_JSON__" not in template or "__WASM_B64__" not in template or "__VERSION__" not in template:
    sys.exit("error: template.html is missing placeholders")

# The version comes from APP_VERSION (the git tag in CI). Local builds show "dev".
version = os.environ.get("APP_VERSION", "").strip() or "dev"

page = template.replace("__GLUE_JSON__", json.dumps(glue))
page = page.replace("__WASM_B64__", wasm_b64)
page = page.replace("__VERSION__", html.escape(version))

out = root.parent / "reduce.html"
out.write_text(page, encoding="utf-8")
print(f"wrote {out} ({out.stat().st_size / 1024:.0f} KB)")
