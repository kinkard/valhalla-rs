#!/usr/bin/env bash
# Profiles one criterion bench and prints self time per symbol.
#
#   scripts/profile.sh 'edgeinfo/way_id_whole_tileset' [seconds]
#   BENCH=tiles_bench TOP=25 scripts/profile.sh 'tile/graph_tile'
#
# Inlined frames collapse into their caller here, because macOS leaves DWARF in the .o files. For
# line level detail run `dsymutil` on the printed binary and open the profile in the samply UI:
# `samply load target/profile/<name>.json.gz`.
set -euo pipefail

filter=${1:?bench filter, e.g. edgeinfo/way_id_whole_tileset}
secs=${2:-10}
bench=${BENCH:-api_bench}

command -v samply >/dev/null || { echo "needs samply: cargo install samply --locked" >&2; exit 1; }

out_dir=target/profile
mkdir -p "$out_dir"
out=$out_dir/$(echo "$filter" | tr '/ ' '__').json.gz

# Full debug info so leaf addresses resolve to functions and lines, without touching Cargo.toml.
CARGO_PROFILE_BENCH_DEBUG=2 cargo bench --no-run --bench "$bench" 2>&1 | tail -1
bin=$(ls -t target/release/deps/"$bench"-* | grep -v '\.' | head -1)

# `--profile-time` runs the bench in a plain loop, with no criterion analysis in the samples.
samply record --save-only -o "$out" -- "$bin" --bench "$filter" --profile-time "$secs" >/dev/null 2>&1

python3 "$(dirname "$0")/self_time.py" "$out" "$bin" "${TOP:-15}"
echo "binary:  $bin"
echo "profile: $out  (samply load "$out" for the UI)"
