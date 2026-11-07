//! The reduction engine: shared by the CLI (`cli.rs`) and the wasm wrapper
//! (`web/`).
//!
//! Everything here is pure computation: parse, build the neighbour table,
//! run greedy/anneal, pad to the requested row count and verify. No I/O.
//! The CLI adds argv parsing, file writing and the printed summary on top;
//! the web wrapper adds JSON.

use crate::anneal::{anneal, Logger};
use crate::clock;
use crate::eval::hit_distribution;
use crate::gformat::format_g;
use crate::greedy::{coverage_counts, dedupe, drop_redundant, greedy_cover};
use crate::neighbours::{ball_size, neighbour_table, too_large_error};
use crate::pad::pad_rows;
use crate::parallel::map_collect;
use crate::system::{parse_system, FullSystem, NUM_MATCHES};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::BTreeMap;

pub const MAX_FILE_ROWS: i64 = 10_000; // Svenska Spel limit for single rows per file
pub const MAX_STAKE_SEK: f64 = 20_000.0; // Svenska Spel limit for total stake
pub const DEFAULT_MAX_NEIGHBOUR_CELLS: i64 = 250_000_000; // ~1 GB of int32
/// Golden-ratio step between greedy-restart RNG seeds.
const RESTART_SEED_STEP: u64 = 0x9E37_79B9_7F4A_7C15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Algorithm {
    Greedy,
    Anneal,
}

impl Algorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Algorithm::Greedy => "greedy",
            Algorithm::Anneal => "anneal",
        }
    }
}

/// One reduction run. Mirrors the CLI options; see the `--help` text in
/// `cli.rs` for the meaning of each field.
pub struct Params {
    pub system: String,
    pub guarantee: i64,
    /// Exact number of rows (`--rows`); pads or truncates as needed.
    pub rows: Option<i64>,
    /// Upper limit on rows (`--max-rows`); may write a partial cover.
    pub max_rows: Option<i64>,
    pub algorithm: Algorithm,
    pub restarts: i64,
    pub iterations: usize,
    /// Anneal: stop searching after this many seconds.
    pub time_limit: Option<f64>,
    pub sample: usize,
    pub t_start: f64,
    pub t_end: f64,
    /// RNG seed (already resolved: the CLI picks a random one if unset).
    pub seed: u64,
    pub max_memory_cells: i64,
    pub row_price: f64,
    /// Compute the hit distribution ("best result") after the run.
    pub hit_enabled: bool,
    /// Operation cap for the hit distribution (work = size x rows x 13).
    pub hit_ops: f64,
}

/// Result of one reduction run: the chosen rows plus everything needed for
/// the summary report (CLI stdout or the web result JSON).
pub struct Outcome {
    /// The full-system model, for rendering rows to text.
    pub fs: FullSystem,
    /// Normalised entries, e.g. `["1", "X", "X2", ...]`.
    pub entries: Vec<String>,
    /// Rows in the full system.
    pub full_size: usize,
    /// `NUM_MATCHES - guarantee`.
    pub radius: usize,
    /// Sphere-covering lower bound, if the neighbour table was built.
    pub lower_bound: Option<i64>,
    /// Rows needed for the full guarantee (best found).
    pub needed: usize,
    /// Row indices written (sorted, deduplicated).
    pub chosen: Vec<u32>,
    /// Outcomes covered by the written rows.
    pub covered: usize,
    /// True when the row limit cut a full cover short.
    pub partial: bool,
    pub elapsed_s: f64,
    pub cost: f64,
    /// "Best result" distribution when enabled and within `hit_ops`.
    pub hit_dist: Option<BTreeMap<u32, u64>>,
    /// True when the hit distribution was enabled but skipped as too large.
    pub hit_skipped: bool,
    /// Svenska Spel limit warnings (row count, total stake).
    pub warnings: Vec<String>,
}

/// File name that shows the input criteria, e.g.
/// `stryktipset_1-X-X2-2-1-1X2-1-X-1-2-1X-1-1X_g12_k100_greedy.txt`
pub fn default_filename(
    product: &str,
    entries: &[String],
    guarantee: i64,
    rows: Option<i64>,
    max_rows: Option<i64>,
    algorithm: Algorithm,
    seed: Option<i64>,
) -> String {
    let mut parts = vec![
        product.to_lowercase(),
        entries.join("-"),
        format!("g{guarantee}"),
    ];
    if let Some(k) = rows {
        parts.push(format!("k{k}"));
    }
    if let Some(n) = max_rows {
        parts.push(format!("n{n}"));
    }
    parts.push(algorithm.as_str().to_string());
    if let Some(s) = seed {
        parts.push(format!("s{s}"));
    }
    format!("{}.txt", parts.join("_"))
}

/// Per-restart RNG seed for the parallel greedy restarts.
fn restart_seed(base: u64, r: usize) -> u64 {
    base.wrapping_add((r as u64).wrapping_mul(RESTART_SEED_STEP))
}

/// Run one reduction. Returns `Err` with the same message text the CLI
/// would print after `error: `.
pub fn reduce(p: &Params, log: &Logger) -> Result<Outcome, String> {
    let entries = parse_system(&p.system)?;
    if !(1..=NUM_MATCHES as i64).contains(&p.guarantee) {
        return Err(format!("--guarantee must be 1..{NUM_MATCHES}"));
    }
    let limit: Option<usize> = {
        let raw = p.rows.or(p.max_rows);
        match raw {
            Some(v) if v < 1 => {
                return Err("--rows/--max-rows must be at least 1".to_string())
            }
            Some(v) => Some(v as usize),
            None => None,
        }
    };

    let base = p.seed;
    // Sequential phases (anneal, padding, full-system sampling) use their own
    // stream so that parallel greedy restarts cannot reorder them.
    let mut main_rng = Pcg64::seed_from_u64(!base);

    let fs = FullSystem::build(entries.clone());
    if let Some(k) = p.rows {
        if k > fs.size as i64 {
            return Err(format!(
                "--rows {} is more than the {} rows of the full system",
                k, fs.size
            ));
        }
    }

    let start_ms = clock::now_ms();
    let deadline: Option<f64> = p
        .time_limit
        .map(|tl| start_ms + tl.max(0.0) * 1000.0);
    let radius = NUM_MATCHES - p.guarantee as usize;
    let chosen: Vec<u32>;
    let needed: usize;
    let covered: usize;

    if radius == 0 || fs.size == 1 {
        needed = fs.size;
        chosen = (0..fs.size as u32).collect();
        let mut chosen = chosen.clone();
        if let Some(lim) = limit {
            if lim < fs.size {
                let mut all: Vec<u32> = (0..fs.size as u32).collect();
                for i in 0..lim {
                    let j = main_rng.random_range(i..all.len());
                    all.swap(i, j);
                }
                all.truncate(lim);
                chosen = all;
            }
        }
        covered = chosen.len();
        return Ok(finish(
            p,
            fs,
            entries,
            radius,
            chosen,
            needed,
            covered,
            None,
            limit,
            start_ms,
        ));
    } else {
        let b = ball_size(&fs.radix, radius);
        if (fs.size as i64) * b > p.max_memory_cells {
            return Err(too_large_error(fs.size, b));
        }
        let nbr = neighbour_table(&fs, radius)?;
        let lb = ((fs.size as i64) + b - 1) / b;
        let lower_bound = Some(lb);
        log.log(&format!(
            "Each row covers {b} rows. Lower bound (sphere covering): {lb} rows"
        ));

        // Greedy with restarts, always to full cover. Restart r gets its own
        // seed; restart 0 keeps Python's deterministic tie-break.
        let restarts = p.restarts.max(1) as usize;
        let results: Vec<Vec<u32>> = map_collect(restarts, |r| {
            let mut rng = Pcg64::seed_from_u64(restart_seed(base, r));
            let cand = greedy_cover(&nbr, &mut rng, None, r > 0, &[]);
            drop_redundant(&nbr, &cand, &mut rng)
        });
        for (r, cand) in results.iter().enumerate() {
            log.log(&format!("  greedy run {}: {} rows", r + 1, cand.len()));
        }
        let mut best: Vec<u32> = results[0].clone();
        for cand in &results[1..] {
            if cand.len() < best.len() {
                best = cand.clone();
            }
        }

        let partial = limit.is_some_and(|l| best.len() > l);
        if p.algorithm == Algorithm::Anneal && !partial {
            // Remove one row at a time and repair the cover with annealing.
            let target_min = ((fs.size as i64) + b - 1) / b;
            let mut current = best.clone();
            while current.len() > target_min as usize
                && deadline.is_none_or(|d| clock::now_ms() < d)
            {
                let cnt = coverage_counts(&nbr, &current);
                // Drop the row that alone covers the fewest outcomes.
                let mut best_j = 0usize;
                let mut best_u = i64::MAX;
                for (j, &c) in current.iter().enumerate() {
                    let u = nbr
                        .ball(c as usize)
                        .iter()
                        .filter(|&&w| cnt[w as usize] == 1)
                        .count() as i64;
                    if u < best_u {
                        best_u = u;
                        best_j = j;
                    }
                }
                let start: Vec<u32> = current
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != best_j)
                    .map(|(_, &c)| c)
                    .collect();
                let (cand, unc) = anneal(
                    &nbr,
                    nbr.n_dist1,
                    &start,
                    p.iterations,
                    &mut main_rng,
                    p.t_start,
                    p.t_end,
                    None,
                    p.sample,
                    deadline,
                );
                if unc != 0 {
                    log.log(&format!(
                        "  anneal: no full cover found with {} rows",
                        start.len()
                    ));
                    break;
                }
                current = drop_redundant(&nbr, &cand, &mut main_rng);
                log.log(&format!("  anneal: full cover with {} rows", current.len()));
            }
            best = current;
        }

        let mut chosen = best.clone();
        needed = best.len();
        if partial {
            let lim = limit.unwrap();
            log.log(&format!(
                "Full guarantee needs about {} rows; limiting to {lim} (partial cover).",
                best.len()
            ));
            // Greedy max-coverage with restarts; keep the best coverage.
            let results: Vec<(Vec<u32>, usize)> = map_collect(restarts, |r| {
                let mut rng = Pcg64::seed_from_u64(restart_seed(base, r));
                let cand = greedy_cover(&nbr, &mut rng, Some(lim), r > 0, &[]);
                let cov = coverage_counts(&nbr, &cand)
                    .iter()
                    .filter(|&&c| c > 0)
                    .count();
                (cand, cov)
            });
            let mut best_cov = results[0].1;
            chosen = results[0].0.clone();
            for (cand, cov) in &results[1..] {
                if *cov > best_cov {
                    best_cov = *cov;
                    chosen = cand.clone();
                }
            }
            if p.algorithm == Algorithm::Anneal {
                let (c, _) = anneal(
                    &nbr,
                    nbr.n_dist1,
                    &chosen,
                    p.iterations,
                    &mut main_rng,
                    p.t_start,
                    p.t_end,
                    Some(log),
                    p.sample,
                    deadline,
                );
                chosen = c;
            }
            chosen = dedupe(&chosen);
            if chosen.len() < lim {
                // anneal may merge duplicate rows
                chosen = greedy_cover(&nbr, &mut main_rng, Some(lim), true, &chosen);
            }
        }
        if let Some(k) = p.rows {
            let k = k as usize;
            if chosen.len() < k {
                log.log(&format!(
                    "Guarantee needs {} rows; adding {} rows to reach {k}.",
                    chosen.len(),
                    k - chosen.len()
                ));
                chosen = pad_rows(
                    &fs, &chosen, k, radius, &mut main_rng, p.max_memory_cells, &log,
                );
            }
        }

        chosen.sort_unstable();
        chosen.dedup();
        covered = coverage_counts(&nbr, &chosen)
            .iter()
            .filter(|&&c| c > 0)
            .count();
        return Ok(finish(
            p,
            fs,
            entries,
            radius,
            chosen,
            needed,
            covered,
            lower_bound,
            limit,
            start_ms,
        ));
    }
}

/// Shared tail for both branches: warnings and the hit distribution.
#[allow(clippy::too_many_arguments)]
fn finish(
    p: &Params,
    fs: FullSystem,
    entries: Vec<String>,
    radius: usize,
    chosen: Vec<u32>,
    needed: usize,
    covered: usize,
    lower_bound: Option<i64>,
    limit: Option<usize>,
    start_ms: f64,
) -> Outcome {
    let elapsed_s = (clock::now_ms() - start_ms) / 1000.0;
    let n_rows = chosen.len();
    let cost = n_rows as f64 * p.row_price;

    let mut warnings: Vec<String> = Vec::new();
    if n_rows as i64 > MAX_FILE_ROWS {
        warnings.push(format!(
            "{n_rows} rows exceeds the file limit of {MAX_FILE_ROWS} single rows"
        ));
    }
    if cost > MAX_STAKE_SEK {
        warnings.push(format!(
            "cost {} SEK exceeds the limit of {} SEK",
            format_g(cost),
            format_g(MAX_STAKE_SEK)
        ));
    }

    let partial = limit.is_some_and(|l| l < needed);
    let (hit_dist, hit_skipped) = if p.hit_enabled {
        match hit_distribution(&fs, &chosen, p.hit_ops) {
            Some(d) => (Some(d), false),
            None => (None, true),
        }
    } else {
        (None, false)
    };
    let full_size = fs.size;

    Outcome {
        fs,
        entries,
        full_size,
        radius,
        lower_bound,
        needed,
        chosen,
        covered,
        partial,
        elapsed_s,
        cost,
        hit_dist,
        hit_skipped,
        warnings,
    }
}
