#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Bounding Box Generation

Generates a 1km-radius buffer polygon around the DE4 4AH centroid in both
BNG (EPSG:27700) and WGS84 (EPSG:4326), exports as GeoJSON.

CONFIDENCE: HIGH
- BNG coordinates are verifiable via OS Places API
- Buffer generation is deterministic geometry
- pyproj/shapely transformations are well-tested
"""

import argparse
import json
import logging
import sys

import geopandas as gpd
from shapely.geometry import Point

from config import (
    CENTROID_BNG,
    CRS_BNG,
    CRS_WGS84,
    DATA_DIR,
    RADIUS_M,
    setup_logging,
)

log = setup_logging("bbox")


def make_buffer_bng(
    centroid: tuple[float, float] = CENTROID_BNG,
    radius_m: float = RADIUS_M,
) -> gpd.GeoDataFrame:
    """
    Create a circular buffer polygon in BNG coordinates.

    CONFIDENCE: HIGH — direct shapely buffer on metric CRS.
    """
    point = Point(centroid)
    buffer = point.buffer(radius_m)
    gdf = gpd.GeoDataFrame(
        {"name": ["study_area"], "radius_m": [radius_m]},
        geometry=[buffer],
        crs=CRS_BNG,
    )
    log.info(
        "Created BNG buffer: centroid=(%d, %d), radius=%dm, area=%.0f m²",
        centroid[0], centroid[1], radius_m, buffer.area,
    )
    return gdf


def to_wgs84(gdf_bng: gpd.GeoDataFrame) -> gpd.GeoDataFrame:
    """
    Reproject BNG GeoDataFrame to WGS84.

    CONFIDENCE: HIGH — pyproj handles BNG↔WGS84 with sub-metre accuracy.
    """
    gdf_wgs = gdf_bng.to_crs(CRS_WGS84)
    bounds = gdf_wgs.total_bounds  # [minx, miny, maxx, maxy]
    log.info(
        "WGS84 bounds: lon=[%.6f, %.6f], lat=[%.6f, %.6f]",
        bounds[0], bounds[2], bounds[1], bounds[3],
    )
    return gdf_wgs


def get_bbox_wgs84(gdf_bng: gpd.GeoDataFrame) -> tuple[float, float, float, float]:
    """
    Return (west, south, east, north) in WGS84 for use with web APIs.

    CONFIDENCE: HIGH.
    """
    gdf_wgs = to_wgs84(gdf_bng)
    bounds = gdf_wgs.total_bounds
    return (bounds[0], bounds[1], bounds[2], bounds[3])


def get_bbox_bng(gdf_bng: gpd.GeoDataFrame) -> tuple[float, float, float, float]:
    """
    Return (minx, miny, maxx, maxy) in BNG.

    CONFIDENCE: HIGH.
    """
    bounds = gdf_bng.total_bounds
    return (bounds[0], bounds[1], bounds[2], bounds[3])


def export_geojson(gdf: gpd.GeoDataFrame, name: str) -> None:
    """Write GeoDataFrame to GeoJSON in the data directory."""
    out = DATA_DIR / f"{name}.geojson"
    gdf.to_file(out, driver="GeoJSON")
    log.info("Exported: %s", out)


def run() -> gpd.GeoDataFrame:
    """Execute the full bounding-box pipeline, return BNG GeoDataFrame."""
    gdf_bng = make_buffer_bng()
    gdf_wgs = to_wgs84(gdf_bng)

    export_geojson(gdf_bng, "study_area_bng")
    export_geojson(gdf_wgs, "study_area_wgs84")

    # Also dump a simple bbox JSON for downstream consumers
    bbox_wgs = get_bbox_wgs84(gdf_bng)
    bbox_bng = get_bbox_bng(gdf_bng)
    bbox_meta = {
        "postcode": "DE4 4AH",
        "centroid_bng": list(CENTROID_BNG),
        "radius_m": RADIUS_M,
        "bbox_bng": list(bbox_bng),
        "bbox_wgs84": list(bbox_wgs),
    }
    meta_path = DATA_DIR / "bbox_metadata.json"
    with open(meta_path, "w") as f:
        json.dump(bbox_meta, f, indent=2)
    log.info("Metadata written: %s", meta_path)

    return gdf_bng


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate study area bounding box")
    parser.add_argument("--radius", type=float, default=RADIUS_M, help="Buffer radius in metres")
    args = parser.parse_args()

    gdf = make_buffer_bng(radius_m=args.radius)
    gdf_wgs = to_wgs84(gdf)
    export_geojson(gdf, "study_area_bng")
    export_geojson(gdf_wgs, "study_area_wgs84")


if __name__ == "__main__":
    main()
