#!/usr/bin/env python3
"""Gate Quest 3 on-device benchmark output against PRD-008 §6 budgets.

Expected report shape (emitted by xr-client/perf/benchmark.gd via logcat):

    {
      "duration_s": 30.0,
      "frame_count": 2700,
      "fps_mean": 89.9,
      "fps_p99": 88.4,
      "cpu_ms_p95": 7.6,
      "gpu_ms_p95": 7.4,
      "draw_calls_max": 47,
      "tri_count_max": 92800
    }

Budgets per PRD-008 §6 / PRD-QE-002 §5.2:
    fps_p99            ≥ target_fps - 2 (target 90 → ≥ 88 p99)
    cpu_ms_p95         ≤ max-cpu-ms
    gpu_ms_p95         ≤ max-gpu-ms
    draw_calls_max     ≤ max-draw-calls
"""

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--report", required=True, type=Path)
    p.add_argument("--target-fps", type=float, required=True)
    p.add_argument("--max-cpu-ms", type=float, required=True)
    p.add_argument("--max-gpu-ms", type=float, required=True)
    p.add_argument("--max-draw-calls", type=int, required=True)
    p.add_argument("--fps-tolerance", type=float, default=2.0)
    args = p.parse_args()

    if not args.report.exists():
        print(f"::error::perf report not found: {args.report}", file=sys.stderr)
        return 1

    try:
        data = json.loads(args.report.read_text())
    except json.JSONDecodeError as exc:
        print(f"::error::invalid perf JSON: {exc}", file=sys.stderr)
        return 1

    fps_p99 = float(data.get("fps_p99", 0))
    cpu_p95 = float(data.get("cpu_ms_p95", 999))
    gpu_p95 = float(data.get("gpu_ms_p95", 999))
    draws = int(data.get("draw_calls_max", 999))

    fps_floor = args.target_fps - args.fps_tolerance

    failures: list[str] = []

    if fps_p99 < fps_floor:
        failures.append(f"fps_p99 {fps_p99:.2f} < {fps_floor:.2f}")
    if cpu_p95 > args.max_cpu_ms:
        failures.append(f"cpu_ms_p95 {cpu_p95:.2f} > {args.max_cpu_ms:.2f}")
    if gpu_p95 > args.max_gpu_ms:
        failures.append(f"gpu_ms_p95 {gpu_p95:.2f} > {args.max_gpu_ms:.2f}")
    if draws > args.max_draw_calls:
        failures.append(f"draw_calls_max {draws} > {args.max_draw_calls}")

    print(f"fps_p99={fps_p99} cpu_ms_p95={cpu_p95} gpu_ms_p95={gpu_p95} draw_calls_max={draws}")

    if failures:
        for f in failures:
            print(f"::error::{f}", file=sys.stderr)
        return 1

    print("On-device perf within budget.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
