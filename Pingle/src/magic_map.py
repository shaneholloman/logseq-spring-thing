#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Natural England MAGIC Map Data

Fetches Priority Habitat Inventory and designated site boundaries
(SSSI, SAC, SPA) from Natural England WFS endpoints.

CONFIDENCE:
- Priority Habitat Inventory (PHI): HIGH for current data
  (published annually by Natural England, comprehensive for England)
- SSSI/SAC/SPA boundaries: HIGH (statutory designations, stable data)
- Historic 2016 data: MEDIUM
  (PHI is updated annually but WFS serves current version only;
   historic versions may need WMS TIME parameter or archive download)
- WFS endpoint stability: HIGH
  (DEFRA Data Services Platform is the canonical open data source)
"""

import argparse
import json
import logging
import sys
from typing import Optional

import geopandas as gpd
import requests
from shapely.geometry import box

from config import (
    CENTROID_BNG,
    CRS_BNG,
    DATA_DIR,
    NE_PHI_WFS,
    NE_SAC_WFS,
    NE_SPA_WFS,
    NE_SSSI_WFS,
    RADIUS_M,
    setup_logging,
)

log = setup_logging("magic_map")


# ---------------------------------------------------------------------------
# WFS query helpers
# ---------------------------------------------------------------------------

def _bbox_filter(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
) -> str:
    """
    Build a BBOX string for WFS queries in BNG.

    CONFIDENCE: HIGH — straightforward arithmetic.
    """
    minx = centroid[0] - radius_m
    miny = centroid[1] - radius_m
    maxx = centroid[0] + radius_m
    maxy = centroid[1] + radius_m
    return f"{minx},{miny},{maxx},{maxy},EPSG:27700"


def fetch_wfs_features(
    wfs_url: str,
    type_name: str,
    bbox: str,
    max_features: int = 1000,
    crs: str = CRS_BNG,
) -> gpd.GeoDataFrame:
    """
    Generic WFS GetFeature query returning a GeoDataFrame.

    CONFIDENCE: HIGH for methodology.
    MEDIUM for specific endpoints (Natural England may change type names).
    """
    params = {
        "service": "WFS",
        "version": "2.0.0",
        "request": "GetFeature",
        "typeNames": type_name,
        "outputFormat": "GEOJSON",
        "srsName": crs,
        "bbox": bbox,
        "count": max_features,
    }

    log.info("WFS query: %s / %s", wfs_url, type_name)
    log.debug("Params: %s", params)

    resp = requests.get(wfs_url, params=params, timeout=60)
    resp.raise_for_status()
    data = resp.json()

    features = data.get("features", [])
    log.info("Received %d features from %s", len(features), type_name)

    if not features:
        return gpd.GeoDataFrame()

    gdf = gpd.GeoDataFrame.from_features(features, crs=crs)
    return gdf


def discover_type_names(wfs_url: str) -> list[str]:
    """
    Fetch WFS capabilities to discover available type names.

    CONFIDENCE: HIGH — standard WFS GetCapabilities.
    """
    params = {
        "service": "WFS",
        "version": "2.0.0",
        "request": "GetCapabilities",
    }
    resp = requests.get(wfs_url, params=params, timeout=30)
    resp.raise_for_status()

    # Parse XML for FeatureType names
    import xml.etree.ElementTree as ET
    root = ET.fromstring(resp.content)

    # WFS 2.0 namespace
    ns = {"wfs": "http://www.opengis.net/wfs/2.0"}
    type_names = []
    for ft in root.findall(".//wfs:FeatureType/wfs:Name", ns):
        type_names.append(ft.text)

    # Fallback: try without namespace
    if not type_names:
        for ft in root.iter():
            if ft.tag.endswith("Name") and ft.text and ":" in ft.text:
                type_names.append(ft.text)

    log.info("Discovered %d type names at %s", len(type_names), wfs_url)
    return type_names


# ---------------------------------------------------------------------------
# Priority Habitat Inventory
# ---------------------------------------------------------------------------

def fetch_priority_habitats(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
) -> gpd.GeoDataFrame:
    """
    Fetch Natural England Priority Habitat Inventory polygons within BBOX.

    The PHI maps all BAP Priority Habitats across England.
    Key fields: Main_Habit (habitat type), Habitat_su (subtype).

    CONFIDENCE: HIGH for current data.
    MEDIUM for spatial completeness (some habitats under-recorded).
    """
    bbox = _bbox_filter(centroid, radius_m)

    # Try to discover the correct type name first
    # CONFIDENCE: MEDIUM — type names can change between API versions
    try:
        type_names = discover_type_names(NE_PHI_WFS)
        # Look for the priority habitat type
        phi_type = None
        for tn in type_names:
            if "priority" in tn.lower() or "habitat" in tn.lower():
                phi_type = tn
                break
        if not phi_type and type_names:
            phi_type = type_names[0]
            log.warning("Using first available type: %s", phi_type)
    except Exception as exc:
        log.warning("Could not discover type names: %s — using default", exc)
        phi_type = "Priority_Habitat_Inventory_England"

    gdf = fetch_wfs_features(NE_PHI_WFS, phi_type, bbox)

    if len(gdf) > 0:
        log.info("Priority habitats found: %d polygons", len(gdf))
        if "Main_Habit" in gdf.columns:
            habitat_counts = gdf["Main_Habit"].value_counts()
            for hab, count in habitat_counts.items():
                log.info("  %s: %d", hab, count)
    else:
        log.warning("No priority habitat polygons found in study area")

    return gdf


# ---------------------------------------------------------------------------
# Designated sites
# ---------------------------------------------------------------------------

def fetch_designated_sites(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
) -> dict[str, gpd.GeoDataFrame]:
    """
    Fetch SSSI, SAC, and SPA boundaries overlapping the study area.

    CONFIDENCE: HIGH — statutory boundaries are well-maintained open data.
    DE4 4AH is near the Derbyshire Dales SAC and several SSSIs.
    """
    bbox = _bbox_filter(centroid, radius_m)
    results = {}

    endpoints = {
        "sssi": (NE_SSSI_WFS, "Sites_of_Special_Scientific_Interest_England"),
        "sac": (NE_SAC_WFS, "Special_Areas_of_Conservation_England"),
        "spa": (NE_SPA_WFS, "Special_Protection_Areas_England"),
    }

    for name, (wfs_url, default_type) in endpoints.items():
        log.info("Fetching %s designations...", name.upper())
        try:
            # Try to discover correct type name
            type_names = discover_type_names(wfs_url)
            type_name = type_names[0] if type_names else default_type
        except Exception:
            type_name = default_type

        try:
            gdf = fetch_wfs_features(wfs_url, type_name, bbox)
            if len(gdf) > 0:
                log.info(
                    "%s: %d designations overlap study area",
                    name.upper(), len(gdf),
                )
                results[name] = gdf
            else:
                log.info("%s: no designations overlap study area", name.upper())
        except Exception as exc:
            log.warning("Failed to fetch %s: %s", name.upper(), exc)

    return results


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> dict[str, gpd.GeoDataFrame]:
    """Execute full MAGIC Map data fetch."""
    results = {}

    # Priority habitats
    phi = fetch_priority_habitats()
    if len(phi) > 0:
        phi_path = DATA_DIR / "ne_priority_habitats.geojson"
        phi.to_file(phi_path, driver="GeoJSON")
        log.info("PHI saved: %s", phi_path)
        results["phi"] = phi

    # Designated sites
    designations = fetch_designated_sites()
    for name, gdf in designations.items():
        out_path = DATA_DIR / f"ne_{name}.geojson"
        gdf.to_file(out_path, driver="GeoJSON")
        log.info("%s saved: %s (%d features)", name.upper(), out_path, len(gdf))
        results[name] = gdf

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch MAGIC Map / Natural England data")
    parser.add_argument("--phi-only", action="store_true", help="Only fetch PHI")
    parser.add_argument("--designations-only", action="store_true", help="Only fetch SSSI/SAC/SPA")
    parser.add_argument("--discover", help="Discover type names at a WFS URL")
    args = parser.parse_args()

    if args.discover:
        names = discover_type_names(args.discover)
        for n in names:
            print(n)
    elif args.phi_only:
        gdf = fetch_priority_habitats()
        if len(gdf) > 0:
            gdf.to_file(DATA_DIR / "ne_priority_habitats.geojson", driver="GeoJSON")
    elif args.designations_only:
        fetch_designated_sites()
    else:
        run()


if __name__ == "__main__":
    main()
