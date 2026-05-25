#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Ordnance Survey Data Hub Client

Fetches MasterMap Topography features and postcode centroid verification
from the OS Data Hub APIs.

CONFIDENCE: MEDIUM
- Requires OS API key (free tier available at osdatahub.os.uk)
- Rate limits apply: 600 transactions/min on free tier
- MasterMap Topography Layer has comprehensive UK coverage
- WFS pagination may be needed for dense areas
- API schema may change with new OS Data Hub versions
"""

import argparse
import json
import logging
import sys
from typing import Any

import geopandas as gpd
import requests
from shapely.geometry import Point, box

from config import (
    CENTROID_BNG,
    CRS_BNG,
    CRS_WGS84,
    DATA_DIR,
    OS_API_KEY,
    OS_FEATURES_API,
    OS_PLACES_API,
    POSTCODE,
    RADIUS_M,
    setup_logging,
)

log = setup_logging("os_data_hub")

# CONFIDENCE: HIGH — OS Features API WFS type names are documented
TOPO_LAYER = "Topography_TopographicArea"
TOPO_LINE_LAYER = "Topography_TopographicLine"


def check_api_key() -> None:
    """Verify OS API key is available."""
    if not OS_API_KEY:
        log.error(
            "OS_DATA_HUB_API_KEY not set. "
            "Register at https://osdatahub.os.uk/ and set the env var."
        )
        raise SystemExit(1)


# ---------------------------------------------------------------------------
# Places API — postcode centroid verification
# ---------------------------------------------------------------------------

def verify_postcode(postcode: str = POSTCODE) -> dict[str, Any]:
    """
    Verify postcode centroid via OS Places API.

    Returns dict with BNG easting/northing and WGS84 lat/lon.

    CONFIDENCE: HIGH for data accuracy.
    MEDIUM for API availability (requires valid key + quota).
    """
    check_api_key()

    url = f"{OS_PLACES_API}/postcode"
    params = {
        "postcode": postcode,
        "key": OS_API_KEY,
        "output_srs": "EPSG:27700",
    }

    log.info("Querying OS Places API for postcode: %s", postcode)
    resp = requests.get(url, params=params, timeout=30)
    resp.raise_for_status()
    data = resp.json()

    if not data.get("results"):
        log.warning("No results for postcode %s", postcode)
        return {}

    # First result header contains the postcode centroid
    header = data["header"]
    results = data["results"]

    # Extract first DPA (Delivery Point Address) or LPI
    first = results[0].get("DPA") or results[0].get("LPI", {})
    centroid_info = {
        "postcode": postcode,
        "easting": float(first.get("X_COORDINATE", 0)),
        "northing": float(first.get("Y_COORDINATE", 0)),
        "total_results": header.get("totalresults", 0),
    }

    log.info(
        "OS Places centroid for %s: E=%.0f, N=%.0f (%d results)",
        postcode, centroid_info["easting"], centroid_info["northing"],
        centroid_info["total_results"],
    )

    # Compare with configured centroid
    # CONFIDENCE: HIGH — this is a direct verification step
    de = abs(centroid_info["easting"] - CENTROID_BNG[0])
    dn = abs(centroid_info["northing"] - CENTROID_BNG[1])
    log.info(
        "Offset from configured centroid: dE=%.0fm, dN=%.0fm",
        de, dn,
    )
    if de > 500 or dn > 500:
        log.warning(
            "Large offset (>500m) from configured centroid — "
            "consider updating CENTROID_BNG in config.py"
        )

    return centroid_info


# ---------------------------------------------------------------------------
# Features API — MasterMap Topography
# ---------------------------------------------------------------------------

def fetch_topography(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
    max_features: int = 5000,
) -> gpd.GeoDataFrame:
    """
    Fetch OS MasterMap Topography features within the study area.

    Uses WFS GetFeature with a BBOX spatial filter.

    CONFIDENCE: MEDIUM
    - Feature count in 1km radius of rural Derbyshire: ~500-2000 polygons
    - Pagination needed if >max_features (unlikely for 1km radius)
    - Feature schema includes make, descriptiveGroup, descriptiveTerm
    """
    check_api_key()

    minx = centroid[0] - radius_m
    miny = centroid[1] - radius_m
    maxx = centroid[0] + radius_m
    maxy = centroid[1] + radius_m
    bbox_str = f"{minx},{miny},{maxx},{maxy},EPSG:27700"

    params = {
        "service": "WFS",
        "version": "2.0.0",
        "request": "GetFeature",
        "typeNames": TOPO_LAYER,
        "outputFormat": "GEOJSON",
        "srsName": "EPSG:27700",
        "bbox": bbox_str,
        "count": max_features,
        "key": OS_API_KEY,
    }

    log.info("Fetching OS Topography features within BBOX: %s", bbox_str)
    resp = requests.get(OS_FEATURES_API, params=params, timeout=60)
    resp.raise_for_status()
    data = resp.json()

    features = data.get("features", [])
    log.info("Received %d topographic features", len(features))

    if not features:
        log.warning("No topographic features returned — check API key and BBOX")
        return gpd.GeoDataFrame()

    gdf = gpd.GeoDataFrame.from_features(features, crs=CRS_BNG)
    log.info("Topography GeoDataFrame: %d rows, columns: %s", len(gdf), list(gdf.columns))

    return gdf


def fetch_topographic_lines(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
    max_features: int = 5000,
) -> gpd.GeoDataFrame:
    """
    Fetch OS MasterMap Topographic Line features (hedgerows, fences, walls).

    CONFIDENCE: MEDIUM — line features include hedgerows but classification
    depends on descriptiveTerm attribute which can be inconsistent.
    """
    check_api_key()

    minx = centroid[0] - radius_m
    miny = centroid[1] - radius_m
    maxx = centroid[0] + radius_m
    maxy = centroid[1] + radius_m
    bbox_str = f"{minx},{miny},{maxx},{maxy},EPSG:27700"

    params = {
        "service": "WFS",
        "version": "2.0.0",
        "request": "GetFeature",
        "typeNames": TOPO_LINE_LAYER,
        "outputFormat": "GEOJSON",
        "srsName": "EPSG:27700",
        "bbox": bbox_str,
        "count": max_features,
        "key": OS_API_KEY,
    }

    log.info("Fetching OS Topographic Line features within BBOX: %s", bbox_str)
    resp = requests.get(OS_FEATURES_API, params=params, timeout=60)
    resp.raise_for_status()
    data = resp.json()

    features = data.get("features", [])
    log.info("Received %d topographic line features", len(features))

    if not features:
        return gpd.GeoDataFrame()

    gdf = gpd.GeoDataFrame.from_features(features, crs=CRS_BNG)
    return gdf


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> dict[str, gpd.GeoDataFrame]:
    """Execute OS Data Hub ingestion pipeline."""
    results = {}

    # Verify postcode centroid
    centroid_info = verify_postcode()
    centroid_path = DATA_DIR / "os_postcode_centroid.json"
    with open(centroid_path, "w") as f:
        json.dump(centroid_info, f, indent=2)
    log.info("Centroid verification saved: %s", centroid_path)

    # Fetch topography
    topo = fetch_topography()
    if len(topo) > 0:
        topo_path = DATA_DIR / "os_topography.geojson"
        topo.to_file(topo_path, driver="GeoJSON")
        log.info("Topography saved: %s (%d features)", topo_path, len(topo))
        results["topography"] = topo

    # Fetch linear features (hedgerows)
    lines = fetch_topographic_lines()
    if len(lines) > 0:
        lines_path = DATA_DIR / "os_topo_lines.geojson"
        lines.to_file(lines_path, driver="GeoJSON")
        log.info("Topo lines saved: %s (%d features)", lines_path, len(lines))
        results["topo_lines"] = lines

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch OS Data Hub data")
    parser.add_argument("--verify-only", action="store_true", help="Only verify postcode")
    parser.add_argument("--postcode", default=POSTCODE, help="Postcode to verify")
    args = parser.parse_args()

    if args.verify_only:
        info = verify_postcode(args.postcode)
        print(json.dumps(info, indent=2))
    else:
        run()


if __name__ == "__main__":
    main()
