#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Aerial Image Downloader

Downloads high-resolution aerial imagery of the study area from
Google Maps Static API at multiple zoom levels, with tile stitching
for full BBOX coverage.

CONFIDENCE:
- Current imagery: HIGH (Google Maps Static API serves recent imagery)
- 2016 historical imagery: SPECULATIVE
  (Google Maps API does NOT serve historical imagery;
   GEE Sentinel-2/Landsat is the only reliable source for T0)
- Tile stitching: HIGH (standard web mercator tile arithmetic)
- API availability: HIGH (requires valid Google Maps API key with Static Maps enabled)
"""

import argparse
import io
import logging
import math
import sys
from pathlib import Path
from typing import Optional

import requests
from PIL import Image

from config import (
    AERIAL_DIR,
    CENTROID_WGS84,
    GOOGLE_MAPS_API_KEY,
    RADIUS_M,
    setup_logging,
)

log = setup_logging("aerial_downloader")

# Google Maps Static API
# CONFIDENCE: HIGH — stable API, well-documented
STATIC_API_URL = "https://maps.googleapis.com/maps/api/staticmap"
MAX_SIZE = 640  # Max dimension for free tier (640x640)


def check_api_key() -> None:
    """Verify Google Maps API key is available."""
    if not GOOGLE_MAPS_API_KEY:
        log.error(
            "GOOGLE_MAPS_API_KEY not set. "
            "Enable Static Maps API at https://console.cloud.google.com/"
        )
        raise SystemExit(1)


# ---------------------------------------------------------------------------
# Single image download
# ---------------------------------------------------------------------------

def download_static_map(
    lat: float,
    lon: float,
    zoom: int,
    size: tuple[int, int] = (MAX_SIZE, MAX_SIZE),
    maptype: str = "satellite",
    scale: int = 2,
) -> Image.Image:
    """
    Download a single Google Maps Static API tile.

    CONFIDENCE: HIGH for API call; image quality depends on zoom and location.
    Scale=2 gives 1280x1280 actual pixels for 640x640 request (free tier).
    """
    params = {
        "center": f"{lat},{lon}",
        "zoom": zoom,
        "size": f"{size[0]}x{size[1]}",
        "maptype": maptype,
        "scale": scale,
        "key": GOOGLE_MAPS_API_KEY,
    }

    resp = requests.get(STATIC_API_URL, params=params, timeout=30)
    resp.raise_for_status()

    img = Image.open(io.BytesIO(resp.content))
    log.info("Downloaded tile: zoom=%d, center=(%.4f, %.4f), size=%s", zoom, lat, lon, img.size)
    return img


# ---------------------------------------------------------------------------
# Tile math for coverage
# ---------------------------------------------------------------------------

def lat_lon_to_tile(lat: float, lon: float, zoom: int) -> tuple[int, int]:
    """
    Convert lat/lon to tile coordinates at given zoom level.

    CONFIDENCE: HIGH — standard Slippy Map tile math.
    """
    n = 2 ** zoom
    x = int((lon + 180.0) / 360.0 * n)
    lat_rad = math.radians(lat)
    y = int((1.0 - math.log(math.tan(lat_rad) + 1.0 / math.cos(lat_rad)) / math.pi) / 2.0 * n)
    return (x, y)


def tile_to_lat_lon(x: int, y: int, zoom: int) -> tuple[float, float]:
    """Convert tile coordinates back to lat/lon (NW corner)."""
    n = 2 ** zoom
    lon = x / n * 360.0 - 180.0
    lat_rad = math.atan(math.sinh(math.pi * (1 - 2 * y / n)))
    lat = math.degrees(lat_rad)
    return (lat, lon)


def meters_per_pixel(lat: float, zoom: int) -> float:
    """Ground resolution in metres/pixel at given latitude and zoom."""
    return 156543.03392 * math.cos(math.radians(lat)) / (2 ** zoom)


# ---------------------------------------------------------------------------
# Multi-tile stitching
# ---------------------------------------------------------------------------

def download_stitched_area(
    lat: float,
    lon: float,
    radius_m: float,
    zoom: int,
) -> Image.Image:
    """
    Download and stitch multiple tiles to cover the full BBOX.

    CONFIDENCE: HIGH for stitching methodology.
    Cost: each tile is one API call (free tier: 28,000/month).
    """
    mpp = meters_per_pixel(lat, zoom)
    # How many pixels to cover the radius in each direction
    pixels_needed = int(radius_m / mpp)
    tile_pixels = MAX_SIZE * 2  # scale=2

    # Number of tiles in each direction
    n_tiles_x = max(1, math.ceil(2 * pixels_needed / tile_pixels) + 1)
    n_tiles_y = max(1, math.ceil(2 * pixels_needed / tile_pixels) + 1)

    log.info(
        "Stitching %dx%d tiles at zoom=%d (%.1f m/px)",
        n_tiles_x, n_tiles_y, zoom, mpp,
    )

    if n_tiles_x * n_tiles_y > 25:
        log.warning(
            "Large tile count (%d) — consider reducing radius or zoom",
            n_tiles_x * n_tiles_y,
        )

    # Calculate offsets in degrees
    # CONFIDENCE: MEDIUM — approximate for small areas (<10km)
    lat_per_tile = (tile_pixels * mpp) / 111320.0
    lon_per_tile = (tile_pixels * mpp) / (111320.0 * math.cos(math.radians(lat)))

    tiles = []
    for row in range(n_tiles_y):
        tile_row = []
        for col in range(n_tiles_x):
            offset_lat = lat + (n_tiles_y / 2 - row - 0.5) * lat_per_tile
            offset_lon = lon + (col - n_tiles_x / 2 + 0.5) * lon_per_tile

            try:
                img = download_static_map(offset_lat, offset_lon, zoom)
                tile_row.append(img)
            except Exception as exc:
                log.warning("Failed to download tile (%d,%d): %s", row, col, exc)
                # Create blank tile as placeholder
                tile_row.append(Image.new("RGB", (tile_pixels, tile_pixels), (200, 200, 200)))

        tiles.append(tile_row)

    # Stitch
    tile_w, tile_h = tiles[0][0].size
    stitched = Image.new("RGB", (n_tiles_x * tile_w, n_tiles_y * tile_h))

    for row_idx, row in enumerate(tiles):
        for col_idx, tile in enumerate(row):
            stitched.paste(tile, (col_idx * tile_w, row_idx * tile_h))

    log.info("Stitched image: %dx%d pixels", stitched.width, stitched.height)
    return stitched


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> dict[int, Path]:
    """
    Download aerial imagery at multiple zoom levels.

    Returns dict of zoom level → file path.
    """
    check_api_key()

    lat, lon = CENTROID_WGS84
    outputs = {}

    for zoom in (14, 16, 18):
        log.info("=== Downloading zoom level %d ===", zoom)
        try:
            img = download_stitched_area(lat, lon, RADIUS_M, zoom)
            filename = f"aerial_z{zoom}.png"
            out_path = AERIAL_DIR / filename
            img.save(out_path, "PNG")
            log.info("Saved: %s", out_path)
            outputs[zoom] = out_path

            # Also save a JPEG version (smaller file size)
            jpg_path = AERIAL_DIR / f"aerial_z{zoom}.jpg"
            img.save(jpg_path, "JPEG", quality=90)
        except Exception as exc:
            log.error("Failed at zoom %d: %s", zoom, exc)

    # Single high-res centre tile for quick reference
    try:
        centre = download_static_map(lat, lon, 17, maptype="satellite")
        centre_path = AERIAL_DIR / "aerial_centre_z17.png"
        centre.save(centre_path, "PNG")
        log.info("Centre tile saved: %s", centre_path)
    except Exception as exc:
        log.warning("Centre tile download failed: %s", exc)

    return outputs


def main() -> None:
    parser = argparse.ArgumentParser(description="Download aerial imagery")
    parser.add_argument("--zoom", type=int, nargs="+", default=[14, 16, 18])
    parser.add_argument("--centre-only", action="store_true", help="Download single centre tile")
    parser.add_argument("--maptype", default="satellite", choices=["satellite", "hybrid"])
    args = parser.parse_args()

    check_api_key()
    lat, lon = CENTROID_WGS84

    if args.centre_only:
        for z in args.zoom:
            img = download_static_map(lat, lon, z, maptype=args.maptype)
            out = AERIAL_DIR / f"aerial_centre_z{z}.png"
            img.save(out, "PNG")
            log.info("Saved: %s", out)
    else:
        for z in args.zoom:
            img = download_stitched_area(lat, lon, RADIUS_M, z)
            out = AERIAL_DIR / f"aerial_z{z}.png"
            img.save(out, "PNG")


if __name__ == "__main__":
    main()
