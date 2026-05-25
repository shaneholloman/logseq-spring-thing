#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Master Pipeline Orchestrator

Runs all BDE modules in dependency order:

  Phase 1: Ingestion (parallel where possible)
    - bbox generation
    - GEE satellite imagery
    - OS Data Hub topography
    - Natural England MAGIC Map
    - Land Registry INSPIRE
    - Aerial imagery download
    - Thesis mining

  Phase 2: Classification + BM4.0 Calculation
    - Habitat classification (T0 + T1)
    - BM4.0 biodiversity unit calculation
    - Delta computation

  Phase 3: Verification
    - Citation verification
    - Spot checker (planning portal cross-examination)

  Phase 4: Figure Generation
    - All matplotlib figures

Logging to ../logs/pipeline.log with per-module logs.

CONFIDENCE: HIGH for orchestration logic.
Individual module confidence varies — see each module's docstring.
"""

import argparse
import logging
import sys
import time
import traceback
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

# Ensure src is on path
sys.path.insert(0, str(Path(__file__).resolve().parent))

from config import DATA_DIR, LOGS_DIR, OUTPUT_DIR, setup_logging

log = setup_logging("pipeline")


# ---------------------------------------------------------------------------
# Phase definitions
# ---------------------------------------------------------------------------

class PhaseResult:
    """Result container for a pipeline module execution."""

    def __init__(self, module: str, success: bool, duration: float, error: str = ""):
        self.module = module
        self.success = success
        self.duration = duration
        self.error = error

    def __repr__(self) -> str:
        status = "OK" if self.success else "FAIL"
        return f"<{self.module}: {status} ({self.duration:.1f}s)>"


def run_module(name: str, func: Callable, *args: Any, **kwargs: Any) -> PhaseResult:
    """Run a single module with timing and error capture."""
    log.info("--- Starting: %s ---", name)
    start = time.monotonic()
    try:
        func(*args, **kwargs)
        duration = time.monotonic() - start
        log.info("--- Completed: %s (%.1fs) ---", name, duration)
        return PhaseResult(name, True, duration)
    except SystemExit:
        duration = time.monotonic() - start
        log.warning("--- %s exited (missing credentials?) (%.1fs) ---", name, duration)
        return PhaseResult(name, False, duration, "SystemExit (likely missing API key)")
    except Exception as exc:
        duration = time.monotonic() - start
        tb = traceback.format_exc()
        log.error("--- FAILED: %s (%.1fs) ---\n%s", name, duration, tb)
        return PhaseResult(name, False, duration, str(exc))


# ---------------------------------------------------------------------------
# Phase 1: Ingestion
# ---------------------------------------------------------------------------

def phase1_ingestion(
    skip_gee: bool = False,
    skip_os: bool = False,
    skip_aerial: bool = False,
    skip_thesis: bool = False,
    parallel: bool = True,
) -> list[PhaseResult]:
    """
    Phase 1: Data ingestion from all sources.

    Some modules can run in parallel (no inter-dependencies).
    bbox must complete first (other modules depend on it).
    """
    log.info("=" * 60)
    log.info("PHASE 1: DATA INGESTION")
    log.info("=" * 60)

    results = []

    # bbox must run first — other modules depend on study area definition
    from bbox import run as bbox_run
    results.append(run_module("bbox", bbox_run))

    if not results[0].success:
        log.error("bbox failed — cannot continue ingestion")
        return results

    # Parallel ingestion tasks
    tasks = {}

    if not skip_gee:
        from gee_ingestion import run as gee_run
        tasks["gee_ingestion"] = gee_run

    if not skip_os:
        from os_data_hub import run as os_run
        tasks["os_data_hub"] = os_run

    from magic_map import run as magic_run
    tasks["magic_map"] = magic_run

    from land_registry import run as lr_run
    tasks["land_registry"] = lr_run

    if not skip_aerial:
        from aerial_downloader import run as aerial_run
        tasks["aerial_downloader"] = aerial_run

    if not skip_thesis:
        from thesis_miner import run as thesis_run
        tasks["thesis_miner"] = thesis_run

    if parallel and len(tasks) > 1:
        log.info("Running %d ingestion tasks in parallel...", len(tasks))
        with ThreadPoolExecutor(max_workers=4) as executor:
            futures = {
                executor.submit(run_module, name, func): name
                for name, func in tasks.items()
            }
            for future in as_completed(futures):
                result = future.result()
                results.append(result)
    else:
        for name, func in tasks.items():
            results.append(run_module(name, func))

    return results


# ---------------------------------------------------------------------------
# Phase 2: Classification + BM4.0
# ---------------------------------------------------------------------------

def phase2_classification() -> list[PhaseResult]:
    """
    Phase 2: Habitat classification and biodiversity metric calculation.

    Depends on Phase 1 outputs (rasters, PHI polygons, designations).
    """
    log.info("=" * 60)
    log.info("PHASE 2: CLASSIFICATION + BM4.0 CALCULATION")
    log.info("=" * 60)

    results = []

    # Classify T0
    from habitat_classifier import run as classify_run
    results.append(run_module("classifier_t0", classify_run, label="t0"))

    # Classify T1
    results.append(run_module("classifier_t1", classify_run, label="t1"))

    # BM4.0 calculation (depends on both classifications)
    from bm4_calculator import run as bm4_run
    results.append(run_module("bm4_calculator", bm4_run))

    return results


# ---------------------------------------------------------------------------
# Phase 3: Verification
# ---------------------------------------------------------------------------

def phase3_verification() -> list[PhaseResult]:
    """
    Phase 3: Cross-verification and quality checks.

    Can run independently of Phase 2 (citation checking).
    Spot checker needs Phase 2 outputs for comparison.
    """
    log.info("=" * 60)
    log.info("PHASE 3: VERIFICATION + SPOT CHECKS")
    log.info("=" * 60)

    results = []

    from citation_verifier import run as citation_run
    results.append(run_module("citation_verifier", citation_run))

    from spot_checker import run as spot_run
    results.append(run_module("spot_checker", spot_run))

    return results


# ---------------------------------------------------------------------------
# Phase 4: Figure generation
# ---------------------------------------------------------------------------

def phase4_figures() -> list[PhaseResult]:
    """
    Phase 4: Generate all publication figures.

    Depends on Phase 2 outputs (classified maps, BU tables, metrics).
    """
    log.info("=" * 60)
    log.info("PHASE 4: FIGURE GENERATION")
    log.info("=" * 60)

    results = []

    from figure_generator import run as fig_run
    results.append(run_module("figure_generator", fig_run))

    return results


# ---------------------------------------------------------------------------
# Full pipeline
# ---------------------------------------------------------------------------

def run_full_pipeline(
    skip_gee: bool = False,
    skip_os: bool = False,
    skip_aerial: bool = False,
    skip_thesis: bool = False,
    skip_classification: bool = False,
    skip_verification: bool = False,
    skip_figures: bool = False,
    parallel: bool = True,
) -> list[PhaseResult]:
    """
    Execute the full BDE pipeline.

    Returns list of all PhaseResults for summary reporting.
    """
    start_time = datetime.now(timezone.utc)
    log.info("=" * 60)
    log.info("BIODIVERSITY DELTA ENGINE — FULL PIPELINE")
    log.info("Started: %s", start_time.isoformat())
    log.info("=" * 60)

    all_results = []

    # Phase 1: Ingestion
    p1 = phase1_ingestion(
        skip_gee=skip_gee,
        skip_os=skip_os,
        skip_aerial=skip_aerial,
        skip_thesis=skip_thesis,
        parallel=parallel,
    )
    all_results.extend(p1)

    # Phase 2: Classification
    if not skip_classification:
        p2 = phase2_classification()
        all_results.extend(p2)
    else:
        log.info("Skipping Phase 2 (classification)")

    # Phase 3: Verification
    if not skip_verification:
        p3 = phase3_verification()
        all_results.extend(p3)
    else:
        log.info("Skipping Phase 3 (verification)")

    # Phase 4: Figures
    if not skip_figures:
        p4 = phase4_figures()
        all_results.extend(p4)
    else:
        log.info("Skipping Phase 4 (figures)")

    # Summary
    end_time = datetime.now(timezone.utc)
    total_duration = (end_time - start_time).total_seconds()

    log.info("")
    log.info("=" * 60)
    log.info("PIPELINE SUMMARY")
    log.info("=" * 60)
    log.info("Duration: %.1fs", total_duration)

    succeeded = [r for r in all_results if r.success]
    failed = [r for r in all_results if not r.success]

    log.info("Succeeded: %d/%d", len(succeeded), len(all_results))
    for r in succeeded:
        log.info("  [OK]   %-25s (%.1fs)", r.module, r.duration)

    if failed:
        log.warning("Failed: %d/%d", len(failed), len(all_results))
        for r in failed:
            log.warning("  [FAIL] %-25s (%.1fs) — %s", r.module, r.duration, r.error[:80])

    log.info("=" * 60)
    log.info("Output: %s", OUTPUT_DIR)
    log.info("Logs:   %s", LOGS_DIR)
    log.info("=" * 60)

    # Write summary JSON
    summary = {
        "started": start_time.isoformat(),
        "finished": end_time.isoformat(),
        "duration_s": total_duration,
        "total_modules": len(all_results),
        "succeeded": len(succeeded),
        "failed": len(failed),
        "modules": [
            {
                "name": r.module,
                "success": r.success,
                "duration_s": round(r.duration, 1),
                "error": r.error,
            }
            for r in all_results
        ],
    }

    import json
    summary_path = LOGS_DIR / "pipeline_summary.json"
    with open(summary_path, "w") as f:
        json.dump(summary, f, indent=2)
    log.info("Summary: %s", summary_path)

    # Exit code: 0 if all succeeded, 1 if any critical module failed
    critical_failures = [
        r for r in failed
        if r.module in ("bbox", "classifier_t0", "classifier_t1", "bm4_calculator")
    ]
    return all_results


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Biodiversity Delta Engine — Master Pipeline",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Phases:
  1. Ingestion  — Fetch satellite imagery, OS data, PHI, Land Registry, aerial
  2. Classify   — UKHab habitat classification + BM4.0 calculation
  3. Verify     — Citation checking + spot checks against published reports
  4. Figures    — Generate all publication figures

Examples:
  # Run everything
  python pipeline.py

  # Skip GEE (no credentials) and run ingestion only
  python pipeline.py --phase 1 --skip-gee

  # Run classification and figures only (assumes data already fetched)
  python pipeline.py --phase 2 4

  # Dry run — just check configuration
  python pipeline.py --dry-run
        """,
    )
    parser.add_argument(
        "--phase", type=int, nargs="+",
        help="Run specific phases only (1=ingestion, 2=classify, 3=verify, 4=figures)",
    )
    parser.add_argument("--skip-gee", action="store_true", help="Skip GEE ingestion")
    parser.add_argument("--skip-os", action="store_true", help="Skip OS Data Hub")
    parser.add_argument("--skip-aerial", action="store_true", help="Skip aerial download")
    parser.add_argument("--skip-thesis", action="store_true", help="Skip thesis mining")
    parser.add_argument("--no-parallel", action="store_true", help="Disable parallel execution")
    parser.add_argument("--dry-run", action="store_true", help="Check config and exit")
    args = parser.parse_args()

    if args.dry_run:
        log.info("Dry run — checking configuration...")
        from config import (
            POSTCODE, CENTROID_BNG, CENTROID_WGS84, RADIUS_M,
            T0_YEAR, T1_YEAR, OS_API_KEY, GOOGLE_MAPS_API_KEY, GEE_PROJECT,
        )
        log.info("Postcode: %s", POSTCODE)
        log.info("BNG: %s, WGS84: %s", CENTROID_BNG, CENTROID_WGS84)
        log.info("Radius: %dm", RADIUS_M)
        log.info("T0: %d, T1: %d", T0_YEAR, T1_YEAR)
        log.info("OS API key: %s", "SET" if OS_API_KEY else "NOT SET")
        log.info("Google Maps API key: %s", "SET" if GOOGLE_MAPS_API_KEY else "NOT SET")
        log.info("GEE project: %s", GEE_PROJECT or "NOT SET")
        log.info("Output: %s", OUTPUT_DIR)
        log.info("Configuration OK.")
        return

    if args.phase:
        results = []
        if 1 in args.phase:
            results.extend(phase1_ingestion(
                skip_gee=args.skip_gee,
                skip_os=args.skip_os,
                skip_aerial=args.skip_aerial,
                skip_thesis=args.skip_thesis,
                parallel=not args.no_parallel,
            ))
        if 2 in args.phase:
            results.extend(phase2_classification())
        if 3 in args.phase:
            results.extend(phase3_verification())
        if 4 in args.phase:
            results.extend(phase4_figures())
    else:
        results = run_full_pipeline(
            skip_gee=args.skip_gee,
            skip_os=args.skip_os,
            skip_aerial=args.skip_aerial,
            skip_thesis=args.skip_thesis,
            parallel=not args.no_parallel,
        )

    # Exit with appropriate code
    critical = [r for r in results if not r.success and r.module in (
        "bbox", "classifier_t0", "classifier_t1", "bm4_calculator",
    )]
    sys.exit(1 if critical else 0)


if __name__ == "__main__":
    main()
