#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — HM Land Registry INSPIRE Polygons

Fetches cadastral boundary polygons from the HMLR INSPIRE WFS.
These provide land ownership parcels which are essential for per-parcel
BNG metric calculations.

CONFIDENCE: HIGH
- INSPIRE WFS is open, no authentication required
- Coverage is comprehensive for registered land in England/Wales
- Data updates monthly
- WFS endpoint has been stable since 2015
- Caveat: unregistered land (rare but possible in rural Derbyshire) has no polygons
"""

import argparse
import json
import logging
import sys

import geopandas as gpd
import requests
from shapely.geometry import box

from config import (
    CENTROID_BNG,
    CRS_BNG,
    CRS_WGS84,
    DATA_DIR,
    LAND_REGISTRY_WFS,
    RADIUS_M,
    setup_logging,
)

log = setup_logging("land_registry")

# CONFIDENCE: HIGH — documented INSPIRE type name
INSPIRE_TYPE = "inspire:CP.CadastralParcel"


def fetch_cadastral_parcels(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
    max_features: int = 2000,
) -> gpd.GeoDataFrame:
    """
    Fetch HMLR INSPIRE cadastral parcels within the study area.

    The INSPIRE WFS uses WGS84 by default. We query in WGS84 then
    reproject to BNG for consistency with other layers.

    CONFIDENCE: HIGH for methodology and data availability.

    Key attributes:
    - INSPIREID: unique parcel identifier
    - geometry: MultiPolygon boundary
    """
    # Convert BNG centroid to approximate WGS84 bbox
    # CONFIDENCE: HIGH — using geopandas for CRS transform
    from shapely.geometry import Point
    import pyproj
    from shapely.ops import transform

    transformer = pyproj.Transformer.from_crs(
        CRS_BNG, CRS_WGS84, always_xy=True
    )

    minx = centroid[0] - radius_m
    miny = centroid[1] - radius_m
    maxx = centroid[0] + radius_m
    maxy = centroid[1] + radius_m

    # Transform corners to WGS84
    lon_min, lat_min = transformer.transform(minx, miny)
    lon_max, lat_max = transformer.transform(maxx, maxy)

    bbox_str = f"{lat_min},{lon_min},{lat_max},{lon_max},EPSG:4326"

    params = {
        "service": "WFS",
        "version": "2.0.0",
        "request": "GetFeature",
        "typeNames": INSPIRE_TYPE,
        "outputFormat": "application/json",
        "srsName": "EPSG:4326",
        "bbox": bbox_str,
        "count": max_features,
    }

    log.info("Fetching HMLR INSPIRE parcels: BBOX=%s", bbox_str)
    resp = requests.get(LAND_REGISTRY_WFS, params=params, timeout=60)
    resp.raise_for_status()

    data = resp.json()
    features = data.get("features", [])
    log.info("Received %d cadastral parcels", len(features))

    if not features:
        log.warning(
            "No cadastral parcels found — this may indicate the BBOX "
            "falls on unregistered land or the WFS returned an error"
        )
        return gpd.GeoDataFrame()

    gdf = gpd.GeoDataFrame.from_features(features, crs=CRS_WGS84)

    # Reproject to BNG
    gdf = gdf.to_crs(CRS_BNG)

    # Calculate area for each parcel
    gdf["area_m2"] = gdf.geometry.area
    gdf["area_ha"] = gdf["area_m2"] / 10000.0

    log.info(
        "Cadastral parcels: %d parcels, total area: %.2f ha",
        len(gdf), gdf["area_ha"].sum(),
    )

    return gdf


def clip_to_study_area(
    parcels: gpd.GeoDataFrame,
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
) -> gpd.GeoDataFrame:
    """
    Clip cadastral parcels to the circular study area buffer.

    Parcels that straddle the boundary are clipped, and areas recalculated.

    CONFIDENCE: HIGH — standard GIS clip operation.
    """
    from shapely.geometry import Point

    study_area = Point(centroid).buffer(radius_m)
    study_gdf = gpd.GeoDataFrame(
        geometry=[study_area], crs=CRS_BNG
    )

    clipped = gpd.clip(parcels, study_gdf)
    clipped["area_m2"] = clipped.geometry.area
    clipped["area_ha"] = clipped["area_m2"] / 10000.0

    log.info(
        "After clipping: %d parcels, total area: %.2f ha",
        len(clipped), clipped["area_ha"].sum(),
    )

    return clipped


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> gpd.GeoDataFrame:
    """Execute Land Registry data fetch pipeline."""
    parcels = fetch_cadastral_parcels()

    if len(parcels) == 0:
        log.warning("No parcels retrieved — pipeline cannot continue for this module")
        return parcels

    # Clip to study area
    clipped = clip_to_study_area(parcels)

    # Export
    raw_path = DATA_DIR / "lr_cadastral_raw.geojson"
    clipped_path = DATA_DIR / "lr_cadastral_clipped.geojson"

    parcels.to_file(raw_path, driver="GeoJSON")
    clipped.to_file(clipped_path, driver="GeoJSON")

    log.info("Raw parcels: %s", raw_path)
    log.info("Clipped parcels: %s", clipped_path)

    # Summary table
    summary = clipped[["area_ha"]].describe()
    log.info("Parcel area summary:\n%s", summary)

    return clipped


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch HMLR INSPIRE cadastral parcels")
    parser.add_argument("--raw", action="store_true", help="Export raw (unclipped) parcels only")
    args = parser.parse_args()

    if args.raw:
        parcels = fetch_cadastral_parcels()
        if len(parcels) > 0:
            parcels.to_file(DATA_DIR / "lr_cadastral_raw.geojson", driver="GeoJSON")
    else:
        run()


if __name__ == "__main__":
    main()
