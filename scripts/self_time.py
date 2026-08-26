#!/usr/bin/env python3
"""Self time per innermost frame from a samply profile, symbolized with atos.

Usage: self_time.py <profile.json.gz> <binary> [top_n]
"""
import gzip, json, subprocess, sys
from collections import Counter

prof = json.load(gzip.open(sys.argv[1]))
binary = sys.argv[2]
top_n = int(sys.argv[3]) if len(sys.argv) > 3 else 20
TEXT_BASE = 0x100000000

for th in prof["threads"]:
    st, ft, fn, strs = th["stackTable"], th["frameTable"], th["funcTable"], th["stringArray"]
    self_time = Counter()
    for stack in th["samples"]["stack"]:
        if stack is None:
            continue
        frame = st["frame"][stack]
        self_time[strs[fn["name"][ft["func"][frame]]]] += 1

    # One atos call for every distinct leaf address.
    addrs = [name for name in self_time if name.startswith("0x")]
    resolved = {}
    if addrs:
        args = [hex(TEXT_BASE + int(a, 16)) for a in addrs]
        out = subprocess.run(["atos", "-o", binary, "-l", hex(TEXT_BASE), *args],
                             capture_output=True, text=True).stdout.splitlines()
        resolved = dict(zip(addrs, out))

    total = sum(self_time.values())
    print(f"{th['name']}: {total} samples\n")
    print(f"{'self%':>6} {'samples':>8}  {'where':22} symbol")
    for name, n in self_time.most_common(top_n):
        sym = resolved.get(name, name).replace(f" (in {binary.split('/')[-1]})", "")
        where = ""
        if sym.endswith(")") and ":" in sym.rsplit("(", 1)[-1]:
            sym, where = sym.rsplit("(", 1)[0].strip(), sym.rsplit("(", 1)[-1][:-1]
        if len(sym) > 70:
            sym = sym[:34] + "..." + sym[-33:]
        print(f"{100 * n / total:5.1f}% {n:8}  {where:22} {sym}")
    print()
