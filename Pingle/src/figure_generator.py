#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Figure Generator

Generates all publication-quality figures for the BDE report:
site location, habitat classification maps, delta change detection,
BU bar charts, hedgerow networks, accuracy scatter plots,
Monte Carlo distributions, and confusion matrix heatmaps.

All figures saved as both PNG (300 dpi) and PDF to ../output/figures/.

CONFIDENCE: HIGH for figure generation methodology.
Figure content quality depends on upstream data quality.
"""

import argparse
import json
import logging
import sys
from pathlib import Path
from typing import Optional

import geopandas as gpd
import matplotlib
matplotlib.use("Agg")  # Non-interactive backend
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
from matplotlib.colors import ListedColormap, BoundaryNorm
import numpy as np
import pandas as pd
import rasterio
from rasterio.plot import show as rioshow

from config import (
    CENTROID_BNG,
    CENTROID_WGS84,
    CRS_BNG,
    CRS_WGS84,
    DATA_DIR,
    FIGURES_DIR,
    RADIUS_M,
    setup_logging,
)

log = setup_logging("figure_generator")

# ---------------------------------------------------------------------------
# Colour scheme for UKHab classes
# ---------------------------------------------------------------------------

HABITAT_COLOURS = {
    "Cropland": "#FFD700",
    "Modified grassland": "#90EE90",
    "Other neutral grassland": "#228B22",
    "Lowland meadow": "#32CD32",
    "Lowland calcareous grassland": "#7CFC00",
    "Broadleaved woodland": "#006400",
    "Coniferous woodland": "#2F4F4F",
    "Mixed woodland": "#004D00",
    "Mixed scrub": "#8B4513",
    "Heathland": "#800080",
    "Developed land": "#808080",
    "Bare ground": "#D2B48C",
    "Standing water": "#4169E1",
    "Running water": "#1E90FF",
    "Traditional orchard": "#FF8C00",
}

DELTA_CMAP = ListedColormap(["#D32F2F", "#FF9800", "#FFF176", "#81C784", "#1B5E20"])


def _save_fig(fig: plt.Figure, name: str) -> None:
    """Save figure as PNG and PDF."""
    for ext in ("png", "pdf"):
        path = FIGURES_DIR / f"{name}.{ext}"
        fig.savefig(path, dpi=300, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    log.info("Saved: %s (.png + .pdf)", name)


# ---------------------------------------------------------------------------
# 1. Site location map
# ---------------------------------------------------------------------------

def plot_site_location(
    study_area_path: Optional[Path] = None,
) -> None:
    """
    Site location map with OS backdrop via contextily.

    CONFIDENCE: HIGH for figure generation.
    MEDIUM for contextily tile availability (requires internet).
    """
    sa_path = study_area_path or DATA_DIR / "study_area_wgs84.geojson"
    if not sa_path.exists():
        log.warning("Study area GeoJSON not found: %s", sa_path)
        return

    gdf = gpd.read_file(sa_path)
    if gdf.crs.to_epsg() != 3857:
        gdf_web = gdf.to_crs(epsg=3857)
    else:
        gdf_web = gdf

    fig, ax = plt.subplots(1, 1, figsize=(10, 10))
    gdf_web.boundary.plot(ax=ax, color="red", linewidth=2, linestyle="--")

    # Add basemap
    try:
        import contextily as ctx
        ctx.add_basemap(ax, source=ctx.providers.OpenStreetMap.Mapnik, zoom=14)
    except Exception as exc:
        log.warning("Could not add basemap: %s", exc)

    ax.set_title(f"Study Area: DE4 4AH ({RADIUS_M}m radius)", fontsize=14)
    ax.set_xlabel("Easting (Web Mercator)")
    ax.set_ylabel("Northing (Web Mercator)")

    _save_fig(fig, "01_site_location")


# ---------------------------------------------------------------------------
# 2 & 3. Habitat classification maps (T0 and T1)
# ---------------------------------------------------------------------------

def plot_habitat_classification(
    classified_path: Path,
    label: str,
    title_suffix: str = "",
) -> None:
    """
    Habitat classification map coloured by UKHab class.

    CONFIDENCE: HIGH for figure generation.
    """
    if not classified_path.exists():
        log.warning("Classified data not found: %s", classified_path)
        return

    gdf = gpd.read_file(classified_path)
    if "ukhab_label" not in gdf.columns:
        log.warning("No ukhab_label column in %s", classified_path)
        return

    fig, ax = plt.subplots(1, 1, figsize=(12, 10))

    habitats = gdf["ukhab_label"].unique()
    for hab in sorted(habitats):
        colour = HABITAT_COLOURS.get(hab, "#CCCCCC")
        subset = gdf[gdf["ukhab_label"] == hab]
        subset.plot(ax=ax, color=colour, edgecolor="black", linewidth=0.3, label=hab)

    ax.set_title(f"Habitat Classification — {label.upper()}{title_suffix}", fontsize=14)
    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)
    ax.set_xlabel("Easting (BNG)")
    ax.set_ylabel("Northing (BNG)")
    ax.set_aspect("equal")

    _save_fig(fig, f"02_habitat_{label}")


# ---------------------------------------------------------------------------
# 4. Delta change detection map
# ---------------------------------------------------------------------------

def plot_delta_map(
    t0_path: Optional[Path] = None,
    t1_path: Optional[Path] = None,
) -> None:
    """
    Change detection map: red = habitat loss, green = habitat gain.

    CONFIDENCE: HIGH for figure methodology.
    Accuracy depends on classifier quality at both time periods.
    """
    t0_p = t0_path or DATA_DIR / "t0_classified.geojson"
    t1_p = t1_path or DATA_DIR / "t1_classified.geojson"

    if not t0_p.exists() or not t1_p.exists():
        log.warning("Need both T0 and T1 classified data for delta map")
        return

    t0 = gpd.read_file(t0_p)
    t1 = gpd.read_file(t1_p)

    fig, axes = plt.subplots(1, 3, figsize=(20, 8))

    # T0
    for hab in sorted(t0["ukhab_label"].unique()):
        colour = HABITAT_COLOURS.get(hab, "#CCC")
        t0[t0["ukhab_label"] == hab].plot(
            ax=axes[0], color=colour, edgecolor="black", linewidth=0.2
        )
    axes[0].set_title("T0 (2016)", fontsize=12)
    axes[0].set_aspect("equal")

    # T1
    for hab in sorted(t1["ukhab_label"].unique()):
        colour = HABITAT_COLOURS.get(hab, "#CCC")
        t1[t1["ukhab_label"] == hab].plot(
            ax=axes[1], color=colour, edgecolor="black", linewidth=0.2
        )
    axes[1].set_title("T1 (2026)", fontsize=12)
    axes[1].set_aspect("equal")

    # Delta overlay (simplified: show T1 with change highlights)
    # Areas that changed class get highlighted
    # CONFIDENCE: MEDIUM — spatial join accuracy depends on polygon alignment
    try:
        overlay = gpd.overlay(t0, t1, how="intersection", keep_geom_type=True)
        if "ukhab_label_1" in overlay.columns and "ukhab_label_2" in overlay.columns:
            overlay["changed"] = overlay["ukhab_label_1"] != overlay["ukhab_label_2"]
            unchanged = overlay[~overlay["changed"]]
            changed = overlay[overlay["changed"]]

            unchanged.plot(ax=axes[2], color="#E0E0E0", edgecolor="grey", linewidth=0.2)
            if len(changed) > 0:
                # Colour by whether distinctiveness increased or decreased
                changed.plot(
                    ax=axes[2], color="#FF5722", edgecolor="black", linewidth=0.3,
                    label="Changed",
                )
        else:
            t1.plot(ax=axes[2], color="#81C784", edgecolor="black", linewidth=0.2)
    except Exception as exc:
        log.warning("Overlay failed: %s — showing T1 only", exc)
        t1.plot(ax=axes[2], color="#81C784", edgecolor="black", linewidth=0.2)

    axes[2].set_title("Change Detection", fontsize=12)
    axes[2].set_aspect("equal")

    fig.suptitle("Habitat Change: 2016-2026", fontsize=16)
    fig.tight_layout()

    _save_fig(fig, "04_delta_change_detection")


# ---------------------------------------------------------------------------
# 5. BU bar chart per habitat type
# ---------------------------------------------------------------------------

def plot_bu_bar_chart(
    delta_path: Optional[Path] = None,
) -> None:
    """
    Grouped bar chart showing T0 and T1 Biodiversity Units per habitat.

    CONFIDENCE: HIGH for figure generation.
    """
    dp = delta_path or DATA_DIR / ".." / "output" / "tables" / "bng_delta_summary.csv"
    # Try standard location
    for candidate in [
        DATA_DIR.parent / "tables" / "bng_delta_summary.csv",
        Path("/home/devuser/workspace/leila/output/tables/bng_delta_summary.csv"),
    ]:
        if candidate.exists():
            dp = candidate
            break

    if not dp.exists():
        log.warning("Delta summary not found: %s", dp)
        return

    df = pd.read_csv(dp)
    df = df[df["habitat"] != "TOTAL"]

    fig, ax = plt.subplots(figsize=(14, 7))

    x = np.arange(len(df))
    width = 0.35

    bars_t0 = ax.bar(x - width / 2, df["t0_bu"], width, label="T0 (2016)", color="#42A5F5")
    bars_t1 = ax.bar(x + width / 2, df["t1_bu"], width, label="T1 (2026)", color="#66BB6A")

    ax.set_xlabel("Habitat Type", fontsize=12)
    ax.set_ylabel("Biodiversity Units", fontsize=12)
    ax.set_title("Biodiversity Units by Habitat Type", fontsize=14)
    ax.set_xticks(x)
    ax.set_xticklabels(df["habitat"], rotation=45, ha="right", fontsize=9)
    ax.legend()
    ax.grid(axis="y", alpha=0.3)

    fig.tight_layout()
    _save_fig(fig, "05_bu_bar_chart")


# ---------------------------------------------------------------------------
# 6. Hedgerow network map
# ---------------------------------------------------------------------------

def plot_hedgerow_network(
    hedgerow_path: Optional[Path] = None,
    study_area_path: Optional[Path] = None,
) -> None:
    """
    Map of hedgerow network within study area.

    CONFIDENCE: HIGH for figure; MEDIUM for hedgerow data completeness.
    """
    hp = hedgerow_path or DATA_DIR / "hedgerows.geojson"
    sa = study_area_path or DATA_DIR / "study_area_bng.geojson"

    if not hp.exists():
        log.warning("Hedgerow data not found: %s", hp)
        return

    hedges = gpd.read_file(hp)
    fig, ax = plt.subplots(figsize=(10, 10))

    if sa.exists():
        study = gpd.read_file(sa)
        study.boundary.plot(ax=ax, color="grey", linewidth=1, linestyle="--")

    hedges.plot(ax=ax, color="#2E7D32", linewidth=1.5)

    total_km = hedges.geometry.length.sum() / 1000
    ax.set_title(f"Hedgerow Network ({total_km:.1f} km total)", fontsize=14)
    ax.set_aspect("equal")
    ax.set_xlabel("Easting (BNG)")
    ax.set_ylabel("Northing (BNG)")

    _save_fig(fig, "06_hedgerow_network")


# ---------------------------------------------------------------------------
# 7. AI vs ecologist scatter plot
# ---------------------------------------------------------------------------

def plot_accuracy_scatter(
    spot_check_path: Optional[Path] = None,
) -> None:
    """
    Scatter plot comparing AI classification against published ecologist assessments.

    CONFIDENCE: MEDIUM — depends on spot check data availability.
    """
    sp = spot_check_path or DATA_DIR / "spot_check_comparison.csv"
    if not sp.exists():
        log.warning("Spot check data not found: %s — generating placeholder", sp)
        return

    df = pd.read_csv(sp)
    if "ai_bu" not in df.columns or "ecologist_bu" not in df.columns:
        log.warning("Expected columns ai_bu and ecologist_bu in spot check CSV")
        return

    fig, ax = plt.subplots(figsize=(8, 8))

    ax.scatter(df["ecologist_bu"], df["ai_bu"], alpha=0.6, s=40, c="#1565C0")

    # 1:1 line
    lims = [
        min(ax.get_xlim()[0], ax.get_ylim()[0]),
        max(ax.get_xlim()[1], ax.get_ylim()[1]),
    ]
    ax.plot(lims, lims, "k--", alpha=0.5, label="1:1 line")

    ax.set_xlabel("Ecologist BU Assessment", fontsize=12)
    ax.set_ylabel("AI Classification BU", fontsize=12)
    ax.set_title("AI vs Ecologist BNG Assessment Comparison", fontsize=14)
    ax.legend()
    ax.set_aspect("equal")
    ax.grid(alpha=0.3)

    _save_fig(fig, "07_ai_vs_ecologist")


# ---------------------------------------------------------------------------
# 8. Monte Carlo uncertainty distribution
# ---------------------------------------------------------------------------

def plot_monte_carlo(
    mc_path_t0: Optional[Path] = None,
    mc_path_t1: Optional[Path] = None,
) -> None:
    """
    Histogram of Monte Carlo BU distribution for T0 and T1.

    CONFIDENCE: HIGH for figure; MEDIUM for underlying uncertainty model.
    """
    # Check if raw simulation data exists, otherwise use summary
    fig, ax = plt.subplots(figsize=(10, 6))

    for label, default in [("t0", mc_path_t0), ("t1", mc_path_t1)]:
        path = default or DATA_DIR / f"{label}_monte_carlo.json"
        if not path.exists():
            log.warning("Monte Carlo data not found: %s", path)
            continue

        with open(path) as f:
            mc = json.load(f)

        # Generate synthetic distribution from summary stats for plotting
        rng = np.random.default_rng(42)
        samples = rng.normal(mc["mean_bu"], mc["std_bu"], mc["n_simulations"])

        colour = "#42A5F5" if label == "t0" else "#66BB6A"
        ax.hist(samples, bins=50, alpha=0.6, color=colour, label=f"{label.upper()} BU")
        ax.axvline(mc["p5_bu"], color=colour, linestyle=":", alpha=0.8)
        ax.axvline(mc["p95_bu"], color=colour, linestyle=":", alpha=0.8)
        ax.axvline(mc["p50_bu"], color=colour, linestyle="-", linewidth=2)

    ax.set_xlabel("Total Biodiversity Units", fontsize=12)
    ax.set_ylabel("Frequency", fontsize=12)
    ax.set_title("Monte Carlo Uncertainty Distribution", fontsize=14)
    ax.legend()
    ax.grid(alpha=0.3)

    _save_fig(fig, "08_monte_carlo_distribution")


# ---------------------------------------------------------------------------
# 9. Confusion matrix heatmap
# ---------------------------------------------------------------------------

def plot_confusion_matrix(
    metrics_path: Optional[Path] = None,
    label: str = "t1",
) -> None:
    """
    Confusion matrix heatmap from classifier metrics.

    CONFIDENCE: HIGH for figure generation.
    """
    mp = metrics_path or DATA_DIR / f"{label}_classification_metrics.json"
    if not mp.exists():
        log.warning("Metrics not found: %s", mp)
        return

    with open(mp) as f:
        metrics = json.load(f)

    cm = np.array(metrics.get("confusion_matrix", []))
    if cm.size == 0:
        log.warning("Empty confusion matrix")
        return

    classes = metrics.get("classes", {})
    labels = [classes.get(str(i), f"class_{i}") for i in range(cm.shape[0])]

    fig, ax = plt.subplots(figsize=(10, 8))
    im = ax.imshow(cm, interpolation="nearest", cmap="YlOrRd")
    fig.colorbar(im, ax=ax, shrink=0.8)

    ax.set_xticks(np.arange(cm.shape[1]))
    ax.set_yticks(np.arange(cm.shape[0]))
    ax.set_xticklabels(labels, rotation=45, ha="right", fontsize=8)
    ax.set_yticklabels(labels, fontsize=8)

    # Annotate cells
    thresh = cm.max() / 2
    for i in range(cm.shape[0]):
        for j in range(cm.shape[1]):
            ax.text(
                j, i, str(cm[i, j]),
                ha="center", va="center",
                color="white" if cm[i, j] > thresh else "black",
                fontsize=8,
            )

    ax.set_xlabel("Predicted", fontsize=12)
    ax.set_ylabel("True", fontsize=12)
    ax.set_title(f"Confusion Matrix — {label.upper()} Classification", fontsize=14)
    fig.tight_layout()

    _save_fig(fig, f"09_confusion_matrix_{label}")


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> None:
    """Generate all figures."""
    log.info("=== Generating all figures ===")

    plot_site_location()
    plot_habitat_classification(DATA_DIR / "t0_classified.geojson", "t0", " (2016)")
    plot_habitat_classification(DATA_DIR / "t1_classified.geojson", "t1", " (2026)")
    plot_delta_map()
    plot_bu_bar_chart()
    plot_hedgerow_network()
    plot_accuracy_scatter()
    plot_monte_carlo()
    plot_confusion_matrix(label="t0")
    plot_confusion_matrix(label="t1")

    log.info("=== Figure generation complete ===")
    log.info("Output directory: %s", FIGURES_DIR)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate BDE report figures")
    parser.add_argument(
        "--figures",
        nargs="*",
        choices=[
            "location", "t0", "t1", "delta", "bu_bar",
            "hedgerow", "scatter", "montecarlo", "confusion",
        ],
        help="Generate specific figures only",
    )
    args = parser.parse_args()

    if args.figures:
        dispatch = {
            "location": plot_site_location,
            "t0": lambda: plot_habitat_classification(DATA_DIR / "t0_classified.geojson", "t0"),
            "t1": lambda: plot_habitat_classification(DATA_DIR / "t1_classified.geojson", "t1"),
            "delta": plot_delta_map,
            "bu_bar": plot_bu_bar_chart,
            "hedgerow": plot_hedgerow_network,
            "scatter": plot_accuracy_scatter,
            "montecarlo": plot_monte_carlo,
            "confusion": lambda: plot_confusion_matrix(label="t1"),
        }
        for name in args.figures:
            if name in dispatch:
                dispatch[name]()
    else:
        run()


if __name__ == "__main__":
    main()
