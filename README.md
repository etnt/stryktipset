# Stryktipset reduced system

Try it in your browser: [https://etnt.github.io/stryktipset](https://etnt.github.io/stryktipset)

This tool makes a smaller Stryktipset bet from a large system. A system
is a set of rows that covers many possible results. A full system can
cost too much. This tool selects fewer rows and still gives you a
guarantee for the rows it keeps.

## What it does

Stryktipset is a football pool from Svenska Spel. Each coupon has 13
matches. For each match you pick `1` (home win), `X` (draw), `2` (away
win), or a combination such as `1X`, `X2` or `1X2`. A combination covers
more than one result for that match.

The **full system** is every row that you can make from your 13 entries.
A single row is one result for each match, for example `1,X,2,1,...`.

A **guarantee** `G` is the minimum number of correct matches. If the real
result is in the full system, at least one row in your reduced system
has `G` or more correct matches. For example, a guarantee of 12 means
that one row has at least 12 of the 13 matches right. If the real result
is not in the full system, the tool makes no promise.

The tool writes the rows to a text file. The file uses the format for
Svenska Spel external system bets. See
[README-external-stryktipset-format.md](README-external-stryktipset-format.md).

## How it works

1. You enter the 13 entries and the guarantee you want.
2. The tool lists every outcome in the full system.
3. The tool selects rows that cover the most outcomes. It has two methods:
   - `greedy` picks the best row at each step. It is fast.
   - `anneal` starts from the greedy result. It then tries to remove rows
     and find a smaller set. It is slower, but it often finds fewer rows.
4. The tool writes the rows to a file. It also prints a report with the
   number of rows, the cost, and how many outcomes the guarantee covers.

## Ways to run it

You have three options.

1. **Web page (easiest).** Open `reduce.html` in a modern browser. The
   page runs on your computer. Nothing is uploaded. The page is tested
   when you open it over HTTP. Opening the file straight from disk may
   also work, but this is not tested.
2. **Command line, Rust version (recommended).** This is the `reduce-rs`
   program. It is faster on large systems and has no Python dependencies.
   See [Command line (Rust)](#command-line-rust) below.
3. **Command line, Python version (legacy).** This is the older
   `reduce_system.py` script. It gives the same kind of output, but it is
   slower on large systems.
   Use it only if you cannot build the Rust version. See
   [Legacy Python script](#legacy-python-script) below.

## Command line (Rust)

You need a Rust toolchain (`cargo`). Install it from https://rustup.rs.

Build the program once:

```sh
cd reduce-rs
cargo build --release
```

Run it. Use quotes around the system:

```sh
reduce-rs/target/release/reduce-rs "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x" -g 12 -k 5
```

The program writes a file in the current folder. The file name shows the
input, for example:

```text
stryktipset_1-X-X2-2-1-1X2-1-X-1-2-1X-1-1X_g12_k5_greedy.txt
```

Upper and lower case letters both work. The most useful options are:

| Option | Meaning |
| --- | --- |
| `-g`, `--guarantee` | Minimum correct matches. The default is 12. Use 13 for the full system. |
| `-k`, `--rows` | Write exactly this number of rows. |
| `-n`, `--max-rows` | Write at most this number of rows. |
| `-a`, `--algorithm` | `greedy` (default) or `anneal`. |
| `--seed` | Use the same random seed to get the same output again. |
| `-o`, `--output` | Set the file name, or the folder for the automatic name. |
| `--product` | `Stryktipset` (default) or `Europatipset`. |
| `--crlf` | Write Windows line endings. |
| `-v`, `--verbose` | Show the search progress. |
| `-q`, `--quiet` | Do not print the report. |

Run `reduce-rs --help` for the full list.

### Limits

The tool keeps a table of near rows in memory. A very large system with
a low guarantee can need too much memory. The tool then stops with an
error. For example, 13 full matches with guarantee 11 can hit this limit.

### Tests

```sh
cd reduce-rs
cargo test
```

## Legacy Python script

`reduce_system.py` is the first version of this tool. Use the Rust
version for new work. The Python script needs Python 3 and the `numpy`
package:

```sh
pip install numpy
python3 reduce_system.py "1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x" -g 12 -k 5
```

The options are almost the same as in the Rust version. Run
`python3 reduce_system.py --help` for the list.

Run the Python tests:

```sh
python3 -m pytest tests
```

## For developers

### Rebuild the web page

The file `reduce.html` is built from the Rust code. You only need this
when you change the Rust code.

One-time setup:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
```

Then build the page:

```sh
cd reduce-rs
./build.sh
```

The script checks the tool versions. It fails if the `wasm-bindgen` CLI
does not match the version in `Cargo.lock`. The output is written to
`reduce.html`, a single file of about 400 KB. `wasm-opt` (from binaryen)
is optional. It makes the embedded module smaller.

### Check the results

```sh
cd reduce-rs
cargo test                # unit tests and CLI golden tests
./tests/golden.sh check   # golden output matrix for any binary
node tests/parity.mjs     # web page output compared with CLI output (run after ./build.sh)
```

The Rust and Python versions use different random number generators.
With the same seed, greedy output can match exactly, but anneal output
does not. Compare the results by row count and guarantee, not by exact
text.

## License

This project is licensed under the Mozilla Public License 2.0 (MPL-2.0). See [LICENSE](LICENSE) for the full text.
