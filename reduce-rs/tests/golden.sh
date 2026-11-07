#!/usr/bin/env bash
# Capture (or with --check, verify) golden outputs for the reduce-rs CLI.
#
# The goldens lock down byte-identical CLI behaviour while the code is
# refactored (engine extraction, feature-gated rayon, wasm clock). Each case
# records stdout, stderr, the exit code and the produced system file. Timings
# and absolute paths are normalised, everything else must match exactly.
#
# Usage:
#   tests/golden.sh capture [path-to-binary]     # (re)generate tests/goldens
#   tests/golden.sh check [path-to-binary]       # diff against tests/goldens
set -euo pipefail

mode="${1:-check}"
bin="${2:-target/release/reduce-rs}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
bin="$(realpath -- "$bin" 2>/dev/null || echo "$root/$bin")"
golden_dir="$root/tests/goldens"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$golden_dir"

# One case per line: name | arguments...
cases=(
  "greedy12|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 -r3 --seed 42 -v"
  "greedy13|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g13 --seed 42"
  "maxrows|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 --max-rows 4 --seed 7"
  "rows10|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 --rows 10 --seed 7"
  "single|1,1,1,1,1,1,1,1,1,1,1,1,1 -g12 --seed 5"
  "crlf|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 -r3 --seed 42 --crlf --product Europatipset"
  "anneal|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 -a anneal --seed 3 -r 2 -i 20000"
  # The time limit cannot trigger on this small input (it finishes in ~0.1 s),
  # so the case only tests that the flag is parsed and the output stays
  # deterministic on any machine speed.
  "annealtl|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 -a anneal --seed 3 -r 2 -i 200000 --time-limit 600"
  "noverify|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 -r3 --seed 42 --no-verify"
  "outopt|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g12 -r3 --seed 42 -o out/custom_name.txt"
  "err_system|1,2 -g12"
  "err_guarantee|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -g0"
  "err_rows|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -k 100"
  "err_rowsneg|1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x -k 0"
  "err_toolarge|1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2,1x2 -g1"
)

fail=0
for entry in "${cases[@]}"; do
  name="${entry%%|*}"
  args="${entry#*|}"
  dir="$work/$name"
  mkdir -p "$dir/out"
  cd "$dir/out"
  set +e
  # shellcheck disable=SC2086
  "$bin" $args >stdout.raw 2>stderr.raw
  code=$?
  set -e
  cd "$root"

  # Normalise: elapsed time and absolute/relative path noise.
  sed -E -e 's/^  Time:          .* s$/  Time:          <TIME> s/' \
         -e "s#$work/#<WORK>/#g" "$dir/out/stdout.raw" >"$dir/stdout" 2>/dev/null || true
  sed -E -e "s#$work/#<WORK>/#g" "$dir/out/stderr.raw" >"$dir/stderr" 2>/dev/null || true
  echo "$code" >"$dir/code"

  # Collect the produced file(s), if any.
  (cd "$dir/out" && find . -name '*.txt' -type f | sort) >"$dir/files"
  : >"$dir/bodies"
  while IFS= read -r f; do
    body="$dir/out/${f#./}"
    echo "--- $f" >>"$dir/bodies"
    sed -E -e "s#$work/#<WORK>/#g" "$body" >>"$dir/bodies"
  done <"$dir/files"

  if [ "$mode" = capture ]; then
    for part in stdout stderr code files bodies; do
      cp "$dir/$part" "$golden_dir/$name.$part"
    done
  else
    for part in stdout stderr code files bodies; do
      if ! diff -u "$golden_dir/$name.$part" "$dir/$part" >"$work/diff" 2>&1; then
        echo "GOLDEN MISMATCH [$name] ($part):"
        sed -n '1,40p' "$work/diff"
        fail=1
      fi
    done
  fi
done

if [ "$mode" = capture ]; then
  echo "captured ${#cases[@]} goldens into tests/goldens (from $(basename "$bin"))"
else
  if [ "$fail" -eq 0 ]; then
    echo "all ${#cases[@]} golden cases match ($(basename "$bin"))"
  else
    echo "golden check FAILED"
    exit 1
  fi
fi
