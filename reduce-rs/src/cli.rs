//! Command-line interface: argv parsing, file output and the printed
//! summary. The actual reduction lives in `engine.rs`.

use crate::anneal::Logger;
use crate::engine::{self, Algorithm, Params};
use crate::gformat::format_g;
use crate::output::write_file;
use clap::Parser;
use rand;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "reduce-rs",
    about = "Create a reduced Stryktipset system of single rows with a guaranteed number of correct matches."
)]
pub struct Args {
    /// 13 comma-separated entries, e.g. "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x"
    pub system: String,

    /// Guaranteed correct matches if the result is in the full system
    /// (13 = no reduction)
    #[arg(short = 'g', long = "guarantee", default_value_t = 12)]
    pub guarantee: i64,

    /// Upper limit on rows. If the guarantee needs more rows, the best
    /// partial cover is written and the real coverage is reported
    #[arg(short = 'n', long = "max-rows", conflicts_with = "rows")]
    pub max_rows: Option<i64>,

    /// Exact number of rows. If the guarantee needs more, the best partial
    /// cover is written; if it needs fewer, extra rows are added that aim
    /// for more correct matches
    #[arg(short = 'k', long = "rows")]
    pub rows: Option<i64>,

    /// Reduction algorithm
    #[arg(short = 'a', long = "algorithm", default_value = "greedy")]
    pub algorithm: Algorithm,

    /// Greedy restarts with random tie-breaks; the smallest result is kept
    #[arg(short = 'r', long = "restarts", default_value_t = 10)]
    pub restarts: i64,

    /// Anneal: iterations per attempt
    #[arg(short = 'i', long = "iterations", default_value_t = 200_000)]
    pub iterations: usize,

    /// Anneal: stop searching after this many seconds
    #[arg(long = "time-limit")]
    pub time_limit: Option<f64>,

    /// Anneal: candidate moves scored per iteration
    #[arg(long = "sample", default_value_t = 12)]
    pub sample: usize,

    /// Anneal: start temperature
    #[arg(long = "t-start", default_value_t = 0.6)]
    pub t_start: f64,

    /// Anneal: end temperature
    #[arg(long = "t-end", default_value_t = 0.1)]
    pub t_end: f64,

    /// Random seed for reproducible output
    #[arg(long = "seed")]
    pub seed: Option<i64>,

    /// Output file, or an existing directory for the automatic name.
    /// Default: an automatic name in the current directory that shows the
    /// input criteria, e.g. stryktipset_1-X-X2-..._g12_k100_greedy.txt
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Product name on the first line of the file
    #[arg(long = "product", default_value = "Stryktipset")]
    pub product: String,

    /// Write Windows line endings (CRLF)
    #[arg(long = "crlf")]
    pub crlf: bool,

    /// Price per row in SEK, used for the cost report
    #[arg(long = "row-price", default_value_t = 1.0)]
    pub row_price: f64,

    /// Skip the hit-distribution report
    #[arg(long = "no-verify")]
    pub no_verify: bool,

    /// Limit for the neighbour table (rows x ball size)
    #[arg(long = "max-memory-cells", default_value_t = engine::DEFAULT_MAX_NEIGHBOUR_CELLS)]
    pub max_memory_cells: i64,

    /// Write search progress to stderr
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Do not print the summary on stdout
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,
}

pub fn resolve_output(args: &Args, entries: &[String]) -> PathBuf {
    let name = engine::default_filename(
        &args.product,
        entries,
        args.guarantee,
        args.rows,
        args.max_rows,
        args.algorithm,
        args.seed,
    );
    match &args.output {
        None => PathBuf::from(name),
        Some(out) => {
            if out.is_dir() {
                out.join(name)
            } else {
                out.clone()
            }
        }
    }
}

/// Entry point: parse args from `argv` and run. Returns the exit code.
pub fn main_with<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = Args::parse_from(argv);
    run_args(args)
}

/// Entry point for the binary: parse args from the environment.
pub fn run() -> i32 {
    let args = Args::parse();
    run_args(args)
}

fn run_args(args: Args) -> i32 {
    let log = Logger {
        enabled: args.verbose,
    };
    let error = |msg: String| -> i32 {
        eprintln!("error: {msg}");
        2
    };
    let warn = |msg: String| eprintln!("warning: {msg}");

    let base: u64 = match args.seed {
        Some(s) => s as u64, // wrapping, like a NumPy seed
        None => rand::random(),
    };

    let params = Params {
        system: args.system.clone(),
        guarantee: args.guarantee,
        rows: args.rows,
        max_rows: args.max_rows,
        algorithm: args.algorithm,
        restarts: args.restarts,
        iterations: args.iterations,
        time_limit: args.time_limit,
        sample: args.sample,
        t_start: args.t_start,
        t_end: args.t_end,
        seed: base,
        max_memory_cells: args.max_memory_cells,
        row_price: args.row_price,
        // --quiet prints no summary, so the hit distribution would be
        // wasted work.
        hit_enabled: !args.no_verify && !args.quiet,
        hit_ops: 3e9,
    };
    let outcome = match engine::reduce(&params, &log) {
        Ok(o) => o,
        Err(e) => return error(e),
    };
    for w in &outcome.warnings {
        warn(w.clone());
    }

    let out = resolve_output(&args, &outcome.entries);
    if let Err(e) = write_file(
        &out,
        &args.product,
        &outcome.fs,
        &outcome.chosen,
        if args.crlf { "\r\n" } else { "\n" },
    ) {
        eprintln!("error: cannot write {}: {e}", out.display());
        return 2;
    }

    if args.quiet {
        return 0;
    }

    print_summary(&args, &outcome, &out);
    0
}

fn print_summary(args: &Args, outcome: &engine::Outcome, out: &PathBuf) {
    let mut summary: Vec<String> = Vec::new();
    summary.push(format!("{} reduced system", args.product));
    summary.push(format!("  System:        {}", outcome.entries.join(",")));
    summary.push(format!("  Full system:   {} rows", outcome.full_size));
    let size_rule = if args.rows.is_some() {
        format!("exactly {} rows (--rows)", args.rows.unwrap())
    } else if args.max_rows.is_some() {
        format!("at most {} rows (--max-rows)", args.max_rows.unwrap())
    } else {
        "as few rows as possible".to_string()
    };
    summary.push(format!(
        "  Guarantee:     {} correct, {size_rule}",
        args.guarantee
    ));
    let algo = match args.seed {
        Some(s) => format!("{}, seed {s}", args.algorithm.as_str()),
        None => args.algorithm.as_str().to_string(),
    };
    summary.push(format!("  Algorithm:     {algo}"));
    let bound = match outcome.lower_bound {
        Some(lb) => format!(" (best found; lower bound {lb})"),
        None => String::new(),
    };
    summary.push(format!(
        "  Rows needed:   {} for the full guarantee{bound}",
        outcome.needed
    ));
    summary.push(format!(
        "  Rows written:  {}, cost {} SEK",
        outcome.chosen.len(),
        format_g(outcome.cost)
    ));
    summary.push(format!(
        "  Guarantee {} holds for {}/{} outcomes ({:.2} %)",
        args.guarantee,
        outcome.covered,
        outcome.full_size,
        100.0 * outcome.covered as f64 / outcome.full_size as f64
    ));
    if !args.no_verify {
        match outcome.hit_dist {
            None => summary.push("  Best result:   skipped (too large)".to_string()),
            Some(ref dist) => {
                summary
                    .push("  Best result over all outcomes of the full system:".to_string());
                for (&correct, &count) in dist.iter().rev() {
                    summary.push(format!(
                        "    {correct:2} correct: {count:8} outcomes ({:6.2} %)",
                        100.0 * count as f64 / outcome.full_size as f64
                    ));
                }
            }
        }
    }
    summary.push(format!("  Time:          {:.2} s", outcome.elapsed_s));
    summary.push(format!("  Output file:   {}", out.display()));
    println!("{}", summary.join("\n"));
}
