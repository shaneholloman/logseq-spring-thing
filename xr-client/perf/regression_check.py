#!/usr/bin/env python3
"""Compare a fresh perf result JSON against the committed baseline.

Inputs accepted:
  --current  Either:
             (a) the Godot benchmark JSON object emitted as
                 "[XR_PERF_RESULT]={...}" in logcat (just the {...} payload), or
             (b) a Criterion benchmark estimates.json from
                 target/criterion/<bench>/new/estimates.json
  --baseline The committed baseline at
             crates/visionclaw-xr-presence/benches/baseline.json
  --update-baseline  Overwrite the baseline with the current run's numbers.
                    For author-then-reviewer workflow only; CI must not pass it.

Exit codes:
  0  no regression beyond budget
  1  regression beyond budget on at least one metric
  2  invalid input / schema mismatch

Output: a markdown comparison table on stdout (suitable for PR comments).
Stdlib only; Python 3.10+.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

GODOT_METRICS: dict[str, dict[str, Any]] = {
    "frame_ms_p50": {
        "baseline_key": "frame_p50_ms",
        "label": "Frame p50 (ms)",
        "lower_is_better": True,
    },
    "frame_ms_p99": {
        "baseline_key": "frame_p99_ms",
        "label": "Frame p99 (ms)",
        "lower_is_better": True,
    },
    "draw_calls_max": {
        "baseline_key": "max_draw_calls",
        "label": "Draw calls (max)",
        "lower_is_better": True,
    },
    "tri_count_max": {
        "baseline_key": "max_triangles",
        "label": "Triangles (max)",
        "lower_is_better": True,
    },
}

CRITERION_BENCH_TO_BASELINE: dict[str, str] = {
    "encode_pose_frame": "encode_pose_frame",
    "decode_pose_frame": "decode_pose_frame",
    "validate_pose": "validate_pose",
    "delta_compute": "delta_compute",
    "decode_position_frame_1k/decode_1000_nodes": "decode_position_frame_1k_ns",
    "presence_0x43_round_trip": "presence_round_trip_ns",
}


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def detect_kind(payload: Any) -> str:
    if isinstance(payload, dict):
        if "mean" in payload and "median" in payload and isinstance(payload.get("mean"), dict):
            return "criterion"
        if "frame_ms_p99" in payload or "fps_p99" in payload or "draw_calls_max" in payload:
            return "godot"
    return "unknown"


def cmp_godot(current: dict, baseline: dict) -> tuple[list[dict], bool]:
    rows: list[dict] = []
    regressed = False
    for cur_key, spec in GODOT_METRICS.items():
        bk = spec["baseline_key"]
        if cur_key not in current or bk not in baseline:
            continue
        cur_val = float(current[cur_key])
        b_entry = baseline[bk]
        b_val = float(b_entry.get("target_max"))
        budget_pct = b_entry.get("regression_budget_pct")
        budget_abs = b_entry.get("regression_budget_abs")
        # Compare against the absolute target, not against a moving baseline:
        # if we are already at-budget there is no headroom to "regress" into.
        delta = cur_val - b_val
        delta_pct = (delta / b_val * 100.0) if b_val != 0 else 0.0
        over_budget = False
        if budget_pct is not None:
            over_budget = cur_val > b_val * (1.0 + float(budget_pct) / 100.0)
        if budget_abs is not None:
            over_budget = over_budget or cur_val > b_val + float(budget_abs)
        if budget_pct is None and budget_abs is None:
            over_budget = cur_val > b_val
        rows.append({
            "metric": spec["label"],
            "current": cur_val,
            "baseline": b_val,
            "delta": delta,
            "delta_pct": delta_pct,
            "budget": _format_budget(budget_pct, budget_abs),
            "status": "FAIL" if over_budget else "PASS",
        })
        if over_budget:
            regressed = True
    return rows, regressed


def cmp_criterion(current: dict, baseline: dict, bench_name: str | None) -> tuple[list[dict], bool]:
    rows: list[dict] = []
    regressed = False
    cur_median_ns = float(current["median"]["point_estimate"])
    name = bench_name or current.get("_bench_name") or ""
    bk = CRITERION_BENCH_TO_BASELINE.get(name)
    if bk is None:
        for k, v in CRITERION_BENCH_TO_BASELINE.items():
            if name.endswith(k):
                bk = v
                break
    if bk is None or bk not in baseline:
        rows.append({
            "metric": name or "(unknown bench)",
            "current": cur_median_ns,
            "baseline": float("nan"),
            "delta": 0.0,
            "delta_pct": 0.0,
            "budget": "n/a",
            "status": "SKIP",
        })
        return rows, False
    b_entry = baseline[bk]
    b_val = float(b_entry.get("median_ns") or b_entry.get("ns_per_iter"))
    budget_pct = float(b_entry.get("regression_budget_pct", 5))
    budget_ns = b_entry.get("budget_ns")
    delta = cur_median_ns - b_val
    delta_pct = (delta / b_val * 100.0) if b_val != 0 else 0.0
    over_budget = cur_median_ns > b_val * (1.0 + budget_pct / 100.0)
    if budget_ns is not None and cur_median_ns > float(budget_ns):
        over_budget = True
    rows.append({
        "metric": f"{bk} (ns)",
        "current": cur_median_ns,
        "baseline": b_val,
        "delta": delta,
        "delta_pct": delta_pct,
        "budget": f"+{budget_pct:.0f}%",
        "status": "FAIL" if over_budget else "PASS",
    })
    if over_budget:
        regressed = True
    return rows, regressed


def _format_budget(pct: float | None, abs_: float | None) -> str:
    parts = []
    if pct is not None:
        parts.append(f"+{pct}%")
    if abs_ is not None:
        parts.append(f"+{abs_}")
    return " or ".join(parts) if parts else "exact"


def render_table(rows: list[dict]) -> str:
    if not rows:
        return "_(no comparable metrics)_\n"
    out = ["| Metric | Current | Baseline | Delta | Δ% | Budget | Status |",
           "|---|---:|---:|---:|---:|---:|:---:|"]
    for r in rows:
        out.append(
            "| {metric} | {current:.3f} | {baseline:.3f} | {delta:+.3f} | {delta_pct:+.2f}% | {budget} | {status} |".format(**r)
        )
    return "\n".join(out) + "\n"


def update_baseline(baseline_path: Path, baseline: dict, current: dict, kind: str, bench_name: str | None) -> None:
    if kind == "godot":
        for cur_key, spec in GODOT_METRICS.items():
            bk = spec["baseline_key"]
            if cur_key in current and bk in baseline and isinstance(baseline[bk], dict):
                baseline[bk]["last_observed"] = current[cur_key]
    elif kind == "criterion":
        name = bench_name or current.get("_bench_name") or ""
        bk = CRITERION_BENCH_TO_BASELINE.get(name)
        if bk and bk in baseline and isinstance(baseline[bk], dict):
            ns = float(current["median"]["point_estimate"])
            baseline[bk]["median_ns"] = ns
            baseline[bk]["ns_per_iter"] = int(round(ns))
            baseline[bk]["lower_ns"] = float(current["median"]["confidence_interval"]["lower_bound"])
            baseline[bk]["upper_ns"] = float(current["median"]["confidence_interval"]["upper_bound"])
    with baseline_path.open("w", encoding="utf-8") as f:
        json.dump(baseline, f, indent=2, sort_keys=False)
        f.write("\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--current", required=True, type=Path,
                    help="Path to fresh perf result JSON (Godot or Criterion).")
    ap.add_argument("--baseline", required=True, type=Path,
                    help="Path to committed baseline.json.")
    ap.add_argument("--bench-name", default=None,
                    help="For Criterion inputs: explicit bench name (e.g. encode_pose_frame).")
    ap.add_argument("--update-baseline", action="store_true",
                    help="Overwrite the baseline with the current run's numbers.")
    args = ap.parse_args()

    if not args.current.exists():
        print(f"error: --current not found: {args.current}", file=sys.stderr)
        return 2
    if not args.baseline.exists():
        print(f"error: --baseline not found: {args.baseline}", file=sys.stderr)
        return 2

    current = load_json(args.current)
    baseline = load_json(args.baseline)
    kind = detect_kind(current)
    if kind == "unknown":
        print("error: could not classify --current as Godot or Criterion JSON", file=sys.stderr)
        return 2

    if kind == "godot":
        rows, regressed = cmp_godot(current, baseline)
    else:
        rows, regressed = cmp_criterion(current, baseline, args.bench_name)

    print(f"### XR perf regression report — `{kind}` input\n")
    print(render_table(rows))
    if regressed:
        print("\n**FAIL** — at least one metric regressed beyond budget.")
    else:
        print("\n**PASS** — no regressions beyond budget.")

    if args.update_baseline:
        update_baseline(args.baseline, baseline, current, kind, args.bench_name)
        print(f"\nbaseline updated at {args.baseline}")

    return 1 if regressed else 0


if __name__ == "__main__":
    sys.exit(main())
