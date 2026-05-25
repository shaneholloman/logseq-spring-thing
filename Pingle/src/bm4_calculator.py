#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Statutory Biodiversity Metric 4.0 Calculator

Implements the full Biodiversity Unit (BU) calculation from DEFRA's
Biodiversity Metric 4.0:

    BU = Area(ha) x Distinctiveness x Condition x Strategic_Significance x Connectivity

Also handles hedgerow units (per km) and watercourse units (per km).
Computes delta (T1 - T0) across all parcels.

CONFIDENCE:
- Formula: HIGH (direct transcription from DEFRA BM4.0 Calculation Tool v4.0)
- Score lookup tables: HIGH (from BM4.0 Appendix B, published March 2023)
- Score assignment from classified habitats: MEDIUM
  (depends on classifier accuracy; condition must be estimated heuristically
   from spectral indices since field survey is not available)
- Strategic significance: MEDIUM
  (assigned based on proximity to designated sites; simplified from full
   Local Nature Recovery Strategy assessment)
- Connectivity: MEDIUM (simplified spatial analysis)
"""

import argparse
import json
import logging
import sys
from pathlib import Path
from typing import Optional

import geopandas as gpd
import numpy as np
import pandas as pd
from shapely.geometry import Point

from config import (
    CENTROID_BNG,
    CONDITION,
    CONNECTIVITY,
    CRS_BNG,
    DATA_DIR,
    DISTINCTIVENESS,
    STRATEGIC_SIGNIFICANCE,
    TABLES_DIR,
    setup_logging,
)

log = setup_logging("bm4_calculator")


# ---------------------------------------------------------------------------
# Condition estimation from spectral indices
# ---------------------------------------------------------------------------

def estimate_condition(
    habitat_type: str,
    mean_ndvi: float,
    ndvi_std: float,
) -> str:
    """
    Estimate habitat condition from NDVI statistics.

    CONFIDENCE: LOW
    This is a heuristic proxy. Real condition assessment requires field survey
    per DEFRA BM4.0 condition sheets. Spectral indices can indicate vegetation
    vigour but cannot assess species composition, structure, or management.

    Mapping rationale:
    - High NDVI (>0.6) with low variance → well-managed, likely Good
    - Medium NDVI (0.4-0.6) → Moderate
    - Low NDVI (<0.4) or high variance → Poor (stressed or heterogeneous)
    - For developed land → N/A
    """
    if habitat_type in ("Developed land", "Sealed surface", "Artificial unvegetated"):
        return "N/A"

    if habitat_type in ("Bare ground",):
        return "Poor"  # CONFIDENCE: HIGH — bare ground is inherently poor condition

    if habitat_type in ("Standing water", "Running water", "Pond"):
        # CONFIDENCE: LOW — water condition needs chemical/biological assessment
        return "Moderate"

    # Grassland and woodland heuristics
    # CONFIDENCE: LOW — these thresholds are approximate
    if mean_ndvi > 0.65 and ndvi_std < 0.1:
        return "Good"
    elif mean_ndvi > 0.55:
        return "Fairly Good"
    elif mean_ndvi > 0.4:
        return "Moderate"
    elif mean_ndvi > 0.25:
        return "Fairly Poor"
    else:
        return "Poor"


def estimate_strategic_significance(
    habitat_gdf: gpd.GeoDataFrame,
    designation_gdfs: dict[str, gpd.GeoDataFrame],
) -> pd.Series:
    """
    Estimate strategic significance based on proximity to designated sites.

    CONFIDENCE: MEDIUM
    - Full assessment should use Local Nature Recovery Strategy (LNRS)
    - LNRS for Derbyshire not yet published (expected 2025-2026)
    - Proxy: if within/adjacent to SSSI/SAC/SPA → High; within 500m → Medium; else Low

    BM4.0 guidance: "Formally identified in local strategy" → High
    """
    significance = pd.Series("Low", index=habitat_gdf.index)

    for desig_name, desig_gdf in designation_gdfs.items():
        if len(desig_gdf) == 0:
            continue

        desig_union = desig_gdf.geometry.union_all()

        # Within designation
        within = habitat_gdf.geometry.intersects(desig_union)
        significance[within] = "High"

        # Within 500m buffer
        buffered = desig_union.buffer(500)
        near = habitat_gdf.geometry.intersects(buffered) & ~within
        significance[near] = significance[near].where(significance[near] == "High", "Medium")

    log.info(
        "Strategic significance: High=%d, Medium=%d, Low=%d",
        (significance == "High").sum(),
        (significance == "Medium").sum(),
        (significance == "Low").sum(),
    )

    return significance


def estimate_connectivity(
    habitat_gdf: gpd.GeoDataFrame,
) -> pd.Series:
    """
    Estimate connectivity based on spatial adjacency of same-type habitats.

    CONFIDENCE: MEDIUM
    - Simplified version: checks if habitat polygon touches another of same type
    - Full BM4.0 connectivity uses 3-tier assessment (ecologically connected,
      species-specific dispersal distances)
    """
    connectivity = pd.Series("Low", index=habitat_gdf.index)

    for idx, row in habitat_gdf.iterrows():
        same_type = habitat_gdf[
            (habitat_gdf["ukhab_label"] == row["ukhab_label"])
            & (habitat_gdf.index != idx)
        ]
        if len(same_type) == 0:
            continue

        # Check adjacency (touching or within 50m)
        buffered = row.geometry.buffer(50)
        adjacent = same_type.geometry.intersects(buffered)
        n_adjacent = adjacent.sum()

        if n_adjacent >= 3:
            connectivity[idx] = "High"
        elif n_adjacent >= 1:
            connectivity[idx] = "Medium"

    return connectivity


# ---------------------------------------------------------------------------
# BU calculation
# ---------------------------------------------------------------------------

def calculate_area_bu(
    habitat_gdf: gpd.GeoDataFrame,
    designation_gdfs: Optional[dict[str, gpd.GeoDataFrame]] = None,
) -> pd.DataFrame:
    """
    Calculate Biodiversity Units for area-based habitats.

    BU = Area(ha) x Distinctiveness x Condition x Strategic_Significance x Connectivity

    CONFIDENCE: HIGH for formula; MEDIUM for input scores.

    Returns DataFrame with full per-parcel breakdown.
    """
    if len(habitat_gdf) == 0:
        log.warning("Empty habitat GeoDataFrame — returning empty BU table")
        return pd.DataFrame()

    df = habitat_gdf.copy()

    # Ensure area is in hectares
    if "area_ha" not in df.columns:
        df["area_ha"] = df.geometry.area / 10000.0

    # Distinctiveness score
    # CONFIDENCE: HIGH for lookup; MEDIUM for habitat label accuracy
    df["distinctiveness_score"] = df["ukhab_label"].map(DISTINCTIVENESS).fillna(2)

    # Condition (estimated from NDVI if available)
    if "mean_ndvi" in df.columns:
        df["condition_label"] = df.apply(
            lambda r: estimate_condition(
                r["ukhab_label"],
                r.get("mean_ndvi", 0.4),
                r.get("ndvi_std", 0.1),
            ),
            axis=1,
        )
    else:
        # Default to Moderate if no NDVI data
        # CONFIDENCE: LOW — conservative assumption
        log.warning("No NDVI data — defaulting all condition to Moderate")
        df["condition_label"] = "Moderate"

    df["condition_score"] = df["condition_label"].map(CONDITION).fillna(0.56)

    # Strategic significance
    if designation_gdfs:
        df["strategic_significance_label"] = estimate_strategic_significance(
            df, designation_gdfs
        )
    else:
        # CONFIDENCE: MEDIUM — DE4 4AH is near SSSI/SAC so Medium is conservative
        log.warning("No designation data — defaulting strategic significance to Medium")
        df["strategic_significance_label"] = "Medium"

    df["strategic_significance_score"] = (
        df["strategic_significance_label"].map(STRATEGIC_SIGNIFICANCE).fillna(1.0)
    )

    # Connectivity
    df["connectivity_label"] = estimate_connectivity(df)
    df["connectivity_score"] = df["connectivity_label"].map(CONNECTIVITY).fillna(1.0)

    # Calculate BU
    # CONFIDENCE: HIGH for formula
    df["biodiversity_units"] = (
        df["area_ha"]
        * df["distinctiveness_score"]
        * df["condition_score"]
        * df["strategic_significance_score"]
        * df["connectivity_score"]
    )

    log.info(
        "Area BU calculated: %.2f total BU across %d parcels (%.2f ha)",
        df["biodiversity_units"].sum(),
        len(df),
        df["area_ha"].sum(),
    )

    return df


def calculate_hedgerow_bu(
    hedgerow_gdf: gpd.GeoDataFrame,
) -> pd.DataFrame:
    """
    Calculate Biodiversity Units for hedgerows (linear features).

    Hedgerow BU = Length(km) x Distinctiveness x Condition x Strategic_Significance

    CONFIDENCE: MEDIUM
    - Hedgerow identification from satellite/OS data is imprecise
    - Length measurement is reliable if features are correctly identified
    - Condition estimation requires field survey ideally
    """
    if len(hedgerow_gdf) == 0:
        log.info("No hedgerow features to calculate")
        return pd.DataFrame()

    df = hedgerow_gdf.copy()
    df["length_km"] = df.geometry.length / 1000.0

    # Default to "Hedgerow" type — refinement needs field survey
    # CONFIDENCE: LOW — cannot distinguish native vs species-rich from remote sensing
    df["hedge_type"] = "Hedgerow"
    df["distinctiveness_score"] = df["hedge_type"].map(DISTINCTIVENESS).fillna(4)

    # Default condition
    df["condition_label"] = "Moderate"  # CONFIDENCE: LOW
    df["condition_score"] = df["condition_label"].map(CONDITION)

    df["strategic_significance_score"] = 1.10  # Medium default

    df["hedgerow_bu"] = (
        df["length_km"]
        * df["distinctiveness_score"]
        * df["condition_score"]
        * df["strategic_significance_score"]
    )

    log.info(
        "Hedgerow BU: %.2f total across %.2f km",
        df["hedgerow_bu"].sum(),
        df["length_km"].sum(),
    )

    return df


def calculate_watercourse_bu(
    water_gdf: gpd.GeoDataFrame,
) -> pd.DataFrame:
    """
    Calculate Biodiversity Units for watercourses (linear features).

    Watercourse BU = Length(km) x Distinctiveness x Condition x Strategic_Significance

    CONFIDENCE: MEDIUM — same caveats as hedgerow but watercourses are
    easier to identify from remote sensing (NDWI).
    """
    if len(water_gdf) == 0:
        log.info("No watercourse features to calculate")
        return pd.DataFrame()

    df = water_gdf.copy()
    df["length_km"] = df.geometry.length / 1000.0

    df["distinctiveness_score"] = 4  # Medium — Running water
    df["condition_label"] = "Moderate"  # CONFIDENCE: LOW
    df["condition_score"] = df["condition_label"].map(CONDITION)
    df["strategic_significance_score"] = 1.10

    df["watercourse_bu"] = (
        df["length_km"]
        * df["distinctiveness_score"]
        * df["condition_score"]
        * df["strategic_significance_score"]
    )

    log.info(
        "Watercourse BU: %.2f total across %.2f km",
        df["watercourse_bu"].sum(),
        df["length_km"].sum(),
    )

    return df


# ---------------------------------------------------------------------------
# Delta computation
# ---------------------------------------------------------------------------

def compute_delta(
    t0_bu: pd.DataFrame,
    t1_bu: pd.DataFrame,
    bu_column: str = "biodiversity_units",
) -> pd.DataFrame:
    """
    Compute biodiversity delta between T0 and T1.

    Summarises by habitat type and computes net change.

    CONFIDENCE: HIGH for arithmetic; MEDIUM for ecological interpretation.
    A positive delta indicates biodiversity net gain.
    """
    def _summarise(df: pd.DataFrame, prefix: str) -> pd.DataFrame:
        if len(df) == 0:
            return pd.DataFrame()
        agg = df.groupby("ukhab_label").agg(
            area_ha=("area_ha", "sum"),
            total_bu=(bu_column, "sum"),
            n_parcels=("area_ha", "count"),
        ).reset_index()
        agg.columns = [
            "habitat",
            f"{prefix}_area_ha",
            f"{prefix}_bu",
            f"{prefix}_n_parcels",
        ]
        return agg

    t0_summary = _summarise(t0_bu, "t0")
    t1_summary = _summarise(t1_bu, "t1")

    if len(t0_summary) == 0 and len(t1_summary) == 0:
        log.warning("Both T0 and T1 are empty — no delta to compute")
        return pd.DataFrame()

    # Outer merge to capture habitats present in only one period
    delta = pd.merge(t0_summary, t1_summary, on="habitat", how="outer").fillna(0)

    delta["delta_area_ha"] = delta["t1_area_ha"] - delta["t0_area_ha"]
    delta["delta_bu"] = delta["t1_bu"] - delta["t0_bu"]
    delta["pct_change_bu"] = np.where(
        delta["t0_bu"] > 0,
        100.0 * delta["delta_bu"] / delta["t0_bu"],
        np.where(delta["t1_bu"] > 0, 100.0, 0.0),
    )

    # Overall summary
    total_t0 = delta["t0_bu"].sum()
    total_t1 = delta["t1_bu"].sum()
    net_delta = total_t1 - total_t0
    pct_net = 100 * net_delta / total_t0 if total_t0 > 0 else 0

    log.info("=" * 60)
    log.info("BIODIVERSITY DELTA SUMMARY")
    log.info("=" * 60)
    log.info("T0 total BU: %.2f", total_t0)
    log.info("T1 total BU: %.2f", total_t1)
    log.info("Net delta:   %.2f (%.1f%%)", net_delta, pct_net)
    log.info(
        "BNG requirement (10%% minimum): %s",
        "MET" if pct_net >= 10 else "NOT MET",
    )
    log.info("=" * 60)

    # Add totals row
    totals = pd.DataFrame([{
        "habitat": "TOTAL",
        "t0_area_ha": delta["t0_area_ha"].sum(),
        "t0_bu": total_t0,
        "t0_n_parcels": delta["t0_n_parcels"].sum(),
        "t1_area_ha": delta["t1_area_ha"].sum(),
        "t1_bu": total_t1,
        "t1_n_parcels": delta["t1_n_parcels"].sum(),
        "delta_area_ha": delta["delta_area_ha"].sum(),
        "delta_bu": net_delta,
        "pct_change_bu": pct_net,
    }])
    delta = pd.concat([delta, totals], ignore_index=True)

    return delta


# ---------------------------------------------------------------------------
# Monte Carlo uncertainty analysis
# ---------------------------------------------------------------------------

def monte_carlo_uncertainty(
    habitat_gdf: gpd.GeoDataFrame,
    n_simulations: int = 1000,
    condition_uncertainty: float = 0.15,
    area_uncertainty: float = 0.05,
) -> dict:
    """
    Propagate classification and condition uncertainties through BU calculation.

    CONFIDENCE: MEDIUM
    - Uses normally distributed perturbations on condition and area
    - Does not model classification misidentification (would need confusion matrix)
    - Provides 5th/50th/95th percentile confidence intervals
    """
    if len(habitat_gdf) == 0:
        return {}

    rng = np.random.default_rng(RF_RANDOM_STATE := 42)
    total_bus = []

    base_areas = habitat_gdf["area_ha"].values
    base_conditions = habitat_gdf["condition_score"].values
    base_distinct = habitat_gdf["distinctiveness_score"].values
    base_strat = habitat_gdf["strategic_significance_score"].values
    base_conn = habitat_gdf["connectivity_score"].values

    for _ in range(n_simulations):
        # Perturb condition score (truncated normal)
        cond_noise = rng.normal(0, condition_uncertainty, len(base_conditions))
        perturbed_cond = np.clip(base_conditions + cond_noise, 0, 1)

        # Perturb area (small uncertainty)
        area_noise = rng.normal(1, area_uncertainty, len(base_areas))
        perturbed_area = base_areas * np.clip(area_noise, 0.8, 1.2)

        bu = perturbed_area * base_distinct * perturbed_cond * base_strat * base_conn
        total_bus.append(bu.sum())

    total_bus = np.array(total_bus)

    result = {
        "n_simulations": n_simulations,
        "mean_bu": float(total_bus.mean()),
        "std_bu": float(total_bus.std()),
        "p5_bu": float(np.percentile(total_bus, 5)),
        "p50_bu": float(np.percentile(total_bus, 50)),
        "p95_bu": float(np.percentile(total_bus, 95)),
        "condition_uncertainty": condition_uncertainty,
        "area_uncertainty": area_uncertainty,
    }

    log.info(
        "Monte Carlo BU: %.2f [%.2f — %.2f] (5th-95th percentile, n=%d)",
        result["p50_bu"], result["p5_bu"], result["p95_bu"], n_simulations,
    )

    return result


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run(
    t0_classified_path: Optional[Path] = None,
    t1_classified_path: Optional[Path] = None,
    designation_paths: Optional[dict[str, Path]] = None,
    hedgerow_path: Optional[Path] = None,
) -> dict:
    """
    Execute the full BM4.0 calculation pipeline.

    Returns dict containing DataFrames and summary statistics.
    """
    results = {}

    # Load classified habitats
    t0_path = t0_classified_path or DATA_DIR / "t0_classified.geojson"
    t1_path = t1_classified_path or DATA_DIR / "t1_classified.geojson"

    # Load designation data for strategic significance
    designation_gdfs = {}
    desig_default = {"sssi": "ne_sssi.geojson", "sac": "ne_sac.geojson", "spa": "ne_spa.geojson"}
    paths = designation_paths or {k: DATA_DIR / v for k, v in desig_default.items()}
    for name, path in paths.items():
        p = Path(path)
        if p.exists():
            designation_gdfs[name] = gpd.read_file(p)
            log.info("Loaded %s designations: %d features", name, len(designation_gdfs[name]))

    # Calculate BU for each time period
    for label, path in [("t0", t0_path), ("t1", t1_path)]:
        if not path.exists():
            log.warning("Classified data not found: %s — skipping %s", path, label)
            continue

        gdf = gpd.read_file(path)
        bu_df = calculate_area_bu(gdf, designation_gdfs if designation_gdfs else None)
        results[f"{label}_bu"] = bu_df

        # Save
        bu_csv = TABLES_DIR / f"{label}_bu_breakdown.csv"
        bu_df.to_csv(bu_csv, index=False)
        log.info("BU breakdown saved: %s", bu_csv)

        # Monte Carlo
        mc = monte_carlo_uncertainty(bu_df)
        results[f"{label}_mc"] = mc
        mc_path = DATA_DIR / f"{label}_monte_carlo.json"
        with open(mc_path, "w") as f:
            json.dump(mc, f, indent=2)

    # Hedgerows
    hedge_path = hedgerow_path or DATA_DIR / "hedgerows.geojson"
    if hedge_path.exists():
        hedges = gpd.read_file(hedge_path)
        hedge_bu = calculate_hedgerow_bu(hedges)
        results["hedgerow_bu"] = hedge_bu
        if len(hedge_bu) > 0:
            hedge_bu.to_csv(TABLES_DIR / "hedgerow_bu.csv", index=False)

    # Delta
    if "t0_bu" in results and "t1_bu" in results:
        delta = compute_delta(results["t0_bu"], results["t1_bu"])
        results["delta"] = delta
        delta_csv = TABLES_DIR / "bng_delta_summary.csv"
        delta.to_csv(delta_csv, index=False)
        log.info("Delta summary saved: %s", delta_csv)

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="BM4.0 Biodiversity Unit Calculator")
    parser.add_argument("--t0", type=Path, help="T0 classified habitat GeoJSON")
    parser.add_argument("--t1", type=Path, help="T1 classified habitat GeoJSON")
    parser.add_argument("--hedgerows", type=Path, help="Hedgerow features GeoJSON")
    parser.add_argument("--mc-sims", type=int, default=1000, help="Monte Carlo simulations")
    args = parser.parse_args()

    run(
        t0_classified_path=args.t0,
        t1_classified_path=args.t1,
        hedgerow_path=args.hedgerows,
    )


if __name__ == "__main__":
    main()
