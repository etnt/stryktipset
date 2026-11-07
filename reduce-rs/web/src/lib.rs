//! wasm-bindgen wrapper around the `reduce_rs` engine for
//! `tools/stryktipset/reduce.html`.
//!
//! All functions take and return JSON strings so the JS side needs no
//! struct plumbing: `JSON.parse` on the way in and out. Errors come back
//! as `{ok: false, error: ...}` instead of thrown exceptions, so the
//! worker code stays simple.

use reduce_rs::anneal::Logger;
use reduce_rs::engine::{self, Algorithm, Params};
use reduce_rs::output::render;
use reduce_rs::system::{parse_system, FullSystem, NUM_MATCHES};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

/// Validate a compact system string.
/// Returns `{ok, entries, size}` or `{ok: false, error}`.
#[wasm_bindgen]
pub fn validate(system: &str) -> String {
    match parse_system(system) {
        Ok(entries) => {
            let fs = FullSystem::build(entries.clone());
            json!({ "ok": true, "entries": entries, "size": fs.size }).to_string()
        }
        Err(e) => json!({ "ok": false, "error": e }).to_string(),
    }
}

/// Size/shape estimate for a system + guarantee, without building the
/// neighbour table. Returns `{ok, size, ball, lower_bound, cells, too_large}`
/// or `{ok: false, error}`. `cells` is the neighbour-table size the engine
/// would need; `too_large` compares it against `max_memory_cells`.
#[wasm_bindgen]
pub fn estimate(system: &str, guarantee: i64, max_memory_cells: i64) -> String {
    let entries = match parse_system(system) {
        Ok(e) => e,
        Err(e) => return json!({ "ok": false, "error": e }).to_string(),
    };
    if !(1..=NUM_MATCHES as i64).contains(&guarantee) {
        return json!({ "ok": false, "error": format!("--guarantee must be 1..{NUM_MATCHES}") })
            .to_string();
    }
    let fs = FullSystem::build(entries);
    let radius = NUM_MATCHES - guarantee as usize;
    let ball = if radius == 0 || fs.size == 1 {
        1
    } else {
        reduce_rs::neighbours::ball_size(&fs.radix, radius)
    };
    let cells = (fs.size as i64).saturating_mul(ball);
    let lower_bound = ((fs.size as i64) + ball - 1) / ball;
    json!({
        "ok": true,
        "size": fs.size,
        "ball": ball,
        "lower_bound": lower_bound,
        "cells": cells,
        "too_large": cells > max_memory_cells,
    })
    .to_string()
}

/// Run a reduction. Input JSON (all fields except `system` optional):
/// `{system, guarantee, rows, max_rows, algorithm, restarts, iterations,
/// time_limit, sample, t_start, t_end, seed, max_memory_cells, row_price,
/// hit, hit_ops, product, crlf}`. `seed` may be a number or a decimal
/// string (the page sends a string so the full signed 64-bit range
/// survives; JS numbers stop being exact above 2^53).
///
/// Returns `{ok, entries, size, guarantee, radius, lower_bound, needed,
/// rows_written, covered, covered_pct, partial, elapsed_s, cost, hit_dist,
/// hit_skipped, warnings, lines, lines_shown, text}` or
/// `{ok: false, error}`. `text` is the finished file in the Svenska Spel
/// externa systemspel format; `lines` is a preview of at most 200 rows.
#[wasm_bindgen]
pub fn reduce(params_json: &str) -> String {
    let v: Value = match serde_json::from_str(params_json) {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "error": format!("bad params JSON: {e}") }).to_string(),
    };
    let get = |key: &str| v.get(key);

    let system = get("system").and_then(Value::as_str).unwrap_or("");
    let algorithm = match get("algorithm").and_then(Value::as_str) {
        Some("anneal") => Algorithm::Anneal,
        _ => Algorithm::Greedy,
    };
    let seed = match get("seed") {
        // The page sends the seed as a decimal string so the full signed
        // 64-bit range survives; JS numbers stop being exact above 2^53.
        Some(Value::String(s)) => s.trim().parse::<i64>().map(|v| v as u64).unwrap_or(0),
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) as u64,
        // No seed given: derive one from the wall clock, like the CLI's
        // random seed, so the run is still reproducible from the report.
        _ => js_sys::Date::now() as u64,
    };
    let product = get("product")
        .and_then(Value::as_str)
        .unwrap_or("Stryktipset")
        .to_string();
    let crlf = get("crlf").and_then(Value::as_bool).unwrap_or(false);
    let newline = if crlf { "\r\n" } else { "\n" };

    let opt_i64 = |key: &str| get(key).and_then(Value::as_i64);
    let params = Params {
        system: system.to_string(),
        guarantee: get("guarantee").and_then(Value::as_i64).unwrap_or(12),
        rows: opt_i64("rows"),
        max_rows: opt_i64("max_rows"),
        algorithm,
        restarts: get("restarts").and_then(Value::as_i64).unwrap_or(10),
        iterations: get("iterations")
            .and_then(Value::as_u64)
            .unwrap_or(200_000) as usize,
        time_limit: get("time_limit").and_then(Value::as_f64),
        sample: get("sample").and_then(Value::as_u64).unwrap_or(12) as usize,
        t_start: get("t_start").and_then(Value::as_f64).unwrap_or(0.6),
        t_end: get("t_end").and_then(Value::as_f64).unwrap_or(0.1),
        seed,
        max_memory_cells: get("max_memory_cells")
            .and_then(Value::as_i64)
            .unwrap_or(engine::DEFAULT_MAX_NEIGHBOUR_CELLS),
        row_price: get("row_price").and_then(Value::as_f64).unwrap_or(1.0),
        hit_enabled: get("hit").and_then(Value::as_bool).unwrap_or(false),
        // The CLI verifies at 3e9 ops; single-threaded wasm stays responsive
        // with a much lower cap (roughly 15-30 s at worst).
        hit_ops: get("hit_ops").and_then(Value::as_f64).unwrap_or(2e8),
    };

    let outcome = match engine::reduce(&params, &Logger { enabled: false }) {
        Ok(o) => o,
        Err(e) => return json!({ "ok": false, "error": e }).to_string(),
    };

    const MAX_PREVIEW_LINES: usize = 200;
    let preview = outcome.chosen.len().min(MAX_PREVIEW_LINES);
    let lines: Vec<String> = outcome.chosen[..preview]
        .iter()
        .map(|&c| outcome.fs.row_signs(c as usize))
        .collect();
    let text = render(&product, &outcome.fs, &outcome.chosen, newline);

    let covered_pct = 100.0 * outcome.covered as f64 / outcome.full_size as f64;
    let hit_dist: Value = match &outcome.hit_dist {
        Some(d) => {
            let mut m = serde_json::Map::new();
            for (&correct, &count) in d.iter() {
                m.insert(correct.to_string(), json!(count));
            }
            Value::Object(m)
        }
        None => Value::Null,
    };

    json!({
        "ok": true,
        "entries": outcome.entries,
        "size": outcome.full_size,
        "guarantee": params.guarantee,
        "radius": outcome.radius,
        "lower_bound": outcome.lower_bound,
        "needed": outcome.needed,
        "rows_written": outcome.chosen.len(),
        "covered": outcome.covered,
        "covered_pct": covered_pct,
        "partial": outcome.partial,
        "elapsed_s": outcome.elapsed_s,
        "cost": outcome.cost,
        "hit_dist": hit_dist,
        "hit_skipped": outcome.hit_skipped,
        "warnings": outcome.warnings,
        "lines": lines,
        "lines_shown": preview,
        "text": text,
        // Report the seed as a decimal string so any 64-bit value round-
        // trips exactly and can be typed back into the form (and into the
        // CLI) to reproduce the run.
        "seed": (params.seed as i64).to_string(),
    })
    .to_string()
}
