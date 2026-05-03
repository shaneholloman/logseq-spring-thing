#!/usr/bin/env python3
"""Compare criterion bencher-format output against a JSON baseline.

PRD-008 §6 budget: wire encode/decode ≤ 1µs/frame ≈ 1000ns.
PRD-QE-002 §5.2 tolerance: ±5% against rolling baseline.

Bencher format lines look like:
    test encode_pose_frame ... bench:         123 ns/iter (+/- 12)

The baseline JSON shape (committed at
crates/visionclaw-xr-presence/benches/baseline.json):

    {
      "encode_pose_frame": { "ns_per_iter": 250, "budget_ns": 1000 },
      "decode_pose_frame": { "ns_per_iter": 320, "budget_ns": 1000 }
    }

Exit status:
  0  all benches within tolerance and under budget
  1  any bench exceeds tolerance vs baseline OR exceeds absolute budget
"""

import argparse
import json
import re
import sys
from pathlib import Path

BENCHER_LINE = re.compile(
    r"^test\s+(?P<name>\S+)\s+\.\.\.\s+bench:\s+(?P<ns>[\d,]+)\s+ns/iter"
)


def parse_bencher(path: Path) -> dict[str, int]:
    out: dict[str, int] = {}
    for raw in path.read_text().splitlines():
        m = BENCHER_LINE.match(raw.strip())
        if not m:
            continue
        out[m.group("name")] = int(m.group("ns").replace(",", ""))
    return out


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--bencher", required=True, type=Path)
    p.add_argument("--baseline", required=True, type=Path)
    p.add_argument("--tolerance-pct", type=float, default=5.0)
    args = p.parse_args()

    if not args.bencher.exists():
        print(f"::error::bencher output not found: {args.bencher}", file=sys.stderr)
        return 1
    if not args.baseline.exists():
        print(f"::error::baseline not found: {args.baseline}", file=sys.stderr)
        return 1

    measured = parse_bencher(args.bencher)
    baseline = json.loads(args.baseline.read_text())

    if not measured:
        print(f"::error::no benchmark lines parsed from {args.bencher}", file=sys.stderr)
        return 1

    failures: list[str] = []
    for name, base in baseline.items():
        # Skip metadata blocks and any baseline rows that don't describe a
        # criterion ns/iter measurement (e.g. perf-scene gates that live in the
        # same baseline file but are checked by check_perf.py).
        if name.startswith("_") or not isinstance(base, dict):
            continue
        if "ns_per_iter" not in base or "budget_ns" not in base:
            continue
        if name not in measured:
            failures.append(f"missing measurement for {name}")
            continue
        actual = measured[name]
        base_ns = int(base["ns_per_iter"])
        budget_ns = int(base["budget_ns"])
        # Per-row tolerance overrides the CLI default if specified.
        tolerance_pct = float(base.get("regression_budget_pct", args.tolerance_pct))
        ratio = (actual - base_ns) / max(base_ns, 1) * 100.0
        budget_used = (actual / max(budget_ns, 1)) * 100.0

        status = "ok"
        if actual > budget_ns:
            failures.append(
                f"{name}: {actual}ns exceeds absolute budget {budget_ns}ns"
            )
            status = "BUDGET"
        if ratio > tolerance_pct:
            failures.append(
                f"{name}: {actual}ns vs baseline {base_ns}ns is +{ratio:.2f}% (>{tolerance_pct}%)"
            )
            status = "REGRESSION"

        print(
            f"{status:>10}  {name}: {actual} ns ({ratio:+.2f}% vs baseline, "
            f"{budget_used:.1f}% of budget)"
        )

    if failures:
        for f in failures:
            print(f"::error::{f}", file=sys.stderr)
        return 1

    print("All benches within tolerance and under budget.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
