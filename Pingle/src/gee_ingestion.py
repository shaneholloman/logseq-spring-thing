#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Google Earth Engine Data Ingestion

Fetches Sentinel-2 L2A cloud-free composites for T0 (2016) and T1 (2026),
with Landsat 8 fallback for cloudy T0 periods. Computes NDVI and NDWI
spectral indices. Exports clipped GeoTIFFs.

CONFIDENCE:
- Sentinel-2 availability for UK from 2016: HIGH
  (S2A launched June 2015; first full UK year = 2016. S2B added March 2017.)
- Landsat 8 fallback: HIGH (operational since 2013, 30m resolution adequate)
- NDVI/NDWI formulas: HIGH (standard remote sensing indices)
- Cloud masking via SCL band: HIGH (well-established for S2 L2A)
- Export to GeoTIFF: HIGH (standard ee.batch.Export workflow)

REQUIRES: Authenticated GEE session. Run `earthengine authenticate` first,
or set GEE_PROJECT env var for service account auth.
"""

import argparse
import logging
import sys
import time
from pathlib import Path

try:
    import ee
except ImportError:
    print("ERROR: earthengine-api not installed. pip install earthengine-api")
    sys.exit(1)

from config import (
    CENTROID_WGS84,
    CLOUD_THRESHOLD,
    DATA_DIR,
    GEE_PROJECT,
    L8_COLLECTION,
    RADIUS_M,
    S2_COLLECTION,
    T0_SEASON,
    T0_YEAR,
    T1_SEASON,
    T1_YEAR,
    setup_logging,
)

log = setup_logging("gee_ingestion")


# ---------------------------------------------------------------------------
# Authentication
# ---------------------------------------------------------------------------

def authenticate_gee() -> None:
    """
    Initialize GEE. Tries project-based auth first, then default credentials.

    CONFIDENCE: HIGH for methodology; depends on user having valid credentials.
    """
    try:
        if GEE_PROJECT:
            ee.Initialize(project=GEE_PROJECT)
        else:
            ee.Initialize()
        log.info("GEE authenticated successfully")
    except ee.EEException as exc:
        log.error("GEE authentication failed: %s", exc)
        log.error(
            "Run 'earthengine authenticate' or set GEE_PROJECT env var.\n"
            "For service accounts, ensure the JSON key is at "
            "~/.config/earthengine/credentials"
        )
        raise SystemExit(1) from exc


# ---------------------------------------------------------------------------
# Region of interest
# ---------------------------------------------------------------------------

def get_roi() -> ee.Geometry:
    """
    Create a circular ROI in GEE from the configured centroid and radius.

    CONFIDENCE: HIGH — ee.Geometry.Point.buffer is exact on the sphere.
    """
    lat, lon = CENTROID_WGS84
    point = ee.Geometry.Point([lon, lat])
    roi = point.buffer(RADIUS_M)
    log.info("ROI: %.4f, %.4f buffered %dm", lat, lon, RADIUS_M)
    return roi


# ---------------------------------------------------------------------------
# Sentinel-2 cloud masking
# ---------------------------------------------------------------------------

def mask_s2_clouds(image: ee.Image) -> ee.Image:
    """
    Mask clouds and cloud shadows using the Scene Classification Layer (SCL).

    SCL classes to mask:
      3 = cloud shadow, 8 = cloud medium probability,
      9 = cloud high probability, 10 = thin cirrus

    CONFIDENCE: HIGH — standard SCL-based masking for S2 L2A.
    """
    scl = image.select("SCL")
    mask = (
        scl.neq(3)   # cloud shadow
        .And(scl.neq(8))   # cloud medium
        .And(scl.neq(9))   # cloud high
        .And(scl.neq(10))  # thin cirrus
    )
    return image.updateMask(mask)


def add_s2_indices(image: ee.Image) -> ee.Image:
    """
    Add NDVI and NDWI bands to a Sentinel-2 image.

    NDVI = (B8 - B4) / (B8 + B4)       — vegetation vigour
    NDWI = (B3 - B8) / (B3 + B8)       — water content

    CONFIDENCE: HIGH — textbook spectral indices.
    """
    ndvi = image.normalizedDifference(["B8", "B4"]).rename("NDVI")
    ndwi = image.normalizedDifference(["B3", "B8"]).rename("NDWI")
    return image.addBands([ndvi, ndwi])


# ---------------------------------------------------------------------------
# Landsat 8 cloud masking (fallback)
# ---------------------------------------------------------------------------

def mask_l8_clouds(image: ee.Image) -> ee.Image:
    """
    Mask clouds in Landsat 8 Collection 2 using QA_PIXEL band.

    CONFIDENCE: HIGH — standard bitwise QA masking for LC08 C02.
    """
    qa = image.select("QA_PIXEL")
    # Bits 3 (cloud) and 4 (cloud shadow) should be 0
    cloud_bit = 1 << 3
    shadow_bit = 1 << 4
    mask = qa.bitwiseAnd(cloud_bit).eq(0).And(qa.bitwiseAnd(shadow_bit).eq(0))
    return image.updateMask(mask)


def add_l8_indices(image: ee.Image) -> ee.Image:
    """
    Add NDVI and NDWI bands to a Landsat 8 image.

    L8 bands: SR_B5 = NIR, SR_B4 = Red, SR_B3 = Green

    CONFIDENCE: HIGH.
    """
    ndvi = image.normalizedDifference(["SR_B5", "SR_B4"]).rename("NDVI")
    ndwi = image.normalizedDifference(["SR_B3", "SR_B5"]).rename("NDWI")
    return image.addBands([ndvi, ndwi])


# ---------------------------------------------------------------------------
# Composite builders
# ---------------------------------------------------------------------------

def build_s2_composite(
    roi: ee.Geometry,
    year: int,
    season: tuple[int, int],
) -> ee.Image:
    """
    Build a cloud-free Sentinel-2 median composite for the given year and season.

    CONFIDENCE: HIGH for 2017+ (dual satellite, abundant revisits).
    MEDIUM for 2016 (single satellite S2A only, ~10-day revisit).
    """
    start = f"{year}-{season[0]:02d}-01"
    end_month = season[1] + 1 if season[1] < 12 else 12
    end = f"{year}-{end_month:02d}-01" if season[1] < 12 else f"{year}-12-31"

    collection = (
        ee.ImageCollection(S2_COLLECTION)
        .filterBounds(roi)
        .filterDate(start, end)
        .filter(ee.Filter.lt("CLOUDY_PIXEL_PERCENTAGE", CLOUD_THRESHOLD))
        .map(mask_s2_clouds)
        .map(add_s2_indices)
    )

    count = collection.size().getInfo()
    log.info("S2 images for %d season (%s to %s): %d", year, start, end, count)

    if count == 0:
        log.warning("No S2 images found for %d — will attempt Landsat 8 fallback", year)
        return None

    # Select key bands for habitat classification
    bands = ["B2", "B3", "B4", "B5", "B6", "B7", "B8", "B8A", "B11", "B12", "NDVI", "NDWI"]
    composite = collection.select(bands).median().clip(roi)
    log.info("S2 composite built for %d with %d images", year, count)
    return composite


def build_l8_composite(
    roi: ee.Geometry,
    year: int,
    season: tuple[int, int],
) -> ee.Image:
    """
    Build a Landsat 8 median composite — fallback for cloudy S2 periods.

    CONFIDENCE: HIGH for availability (L8 operational since 2013).
    Resolution is 30m vs S2's 10m, so classification accuracy may be lower.
    """
    start = f"{year}-{season[0]:02d}-01"
    end_month = season[1] + 1 if season[1] < 12 else 12
    end = f"{year}-{end_month:02d}-01" if season[1] < 12 else f"{year}-12-31"

    collection = (
        ee.ImageCollection(L8_COLLECTION)
        .filterBounds(roi)
        .filterDate(start, end)
        .filter(ee.Filter.lt("CLOUD_COVER", CLOUD_THRESHOLD))
        .map(mask_l8_clouds)
        .map(add_l8_indices)
    )

    count = collection.size().getInfo()
    log.info("L8 images for %d season (%s to %s): %d", year, start, end, count)

    if count == 0:
        log.error("No Landsat 8 images available either for %d", year)
        return None

    bands = ["SR_B2", "SR_B3", "SR_B4", "SR_B5", "SR_B6", "SR_B7", "NDVI", "NDWI"]
    composite = collection.select(bands).median().clip(roi)
    log.info("L8 composite built for %d with %d images", year, count)
    return composite


# ---------------------------------------------------------------------------
# Export
# ---------------------------------------------------------------------------

def export_to_drive(
    image: ee.Image,
    roi: ee.Geometry,
    description: str,
    scale: int = 10,
    folder: str = "leila_exports",
) -> ee.batch.Task:
    """
    Export image to Google Drive as GeoTIFF.

    CONFIDENCE: HIGH — standard GEE export workflow.
    Note: exports go to Drive, not local filesystem. User must download.
    For automated local download, use ee.data.getDownloadUrl (small areas only).
    """
    task = ee.batch.Export.image.toDrive(
        image=image,
        description=description,
        folder=folder,
        region=roi,
        scale=scale,
        crs="EPSG:27700",
        maxPixels=1e9,
        fileFormat="GeoTIFF",
    )
    task.start()
    log.info("Export task started: %s (check GEE task manager)", description)
    return task


def download_small_region(
    image: ee.Image,
    roi: ee.Geometry,
    filename: str,
    scale: int = 10,
) -> Path:
    """
    Download a small region directly via ee.data.computePixels (for areas < ~10km²).

    CONFIDENCE: MEDIUM — computePixels has payload size limits; 1km radius at 10m
    resolution = ~31k pixels, well within limits.
    """
    import numpy as np
    import rasterio
    from rasterio.transform import from_bounds

    # Get bounding box
    coords = roi.bounds().getInfo()["coordinates"][0]
    lons = [c[0] for c in coords]
    lats = [c[1] for c in coords]
    bbox = [min(lons), min(lats), max(lons), max(lats)]

    # Fetch as numpy array via getDownloadURL
    band_names = image.bandNames().getInfo()
    url = image.getDownloadURL({
        "scale": scale,
        "crs": "EPSG:27700",
        "region": roi,
        "format": "GEO_TIFF",
    })

    import requests
    response = requests.get(url, timeout=300)
    response.raise_for_status()

    out_path = DATA_DIR / filename
    with open(out_path, "wb") as f:
        f.write(response.content)

    log.info("Downloaded: %s (%d bytes)", out_path, len(response.content))
    return out_path


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> dict[str, Path]:
    """
    Execute the full GEE ingestion pipeline.

    Returns dict of output file paths.
    """
    authenticate_gee()
    roi = get_roi()
    outputs = {}

    for label, year, season in [
        ("t0", T0_YEAR, T0_SEASON),
        ("t1", T1_YEAR, T1_SEASON),
    ]:
        log.info("=== Building composite for %s (%d) ===", label.upper(), year)

        composite = build_s2_composite(roi, year, season)
        sensor = "s2"

        if composite is None:
            log.warning("Falling back to Landsat 8 for %s", label)
            composite = build_l8_composite(roi, year, season)
            sensor = "l8"

        if composite is None:
            log.error("No imagery available for %s (%d) — skipping", label, year)
            continue

        filename = f"{label}_{sensor}_composite.tif"
        try:
            path = download_small_region(composite, roi, filename)
            outputs[label] = path
        except Exception as exc:
            log.warning(
                "Direct download failed (%s), falling back to Drive export: %s",
                exc, label,
            )
            task = export_to_drive(
                composite, roi,
                description=f"leila_{label}_{sensor}_{year}",
                scale=10 if sensor == "s2" else 30,
            )
            outputs[f"{label}_task"] = task

    return outputs


def main() -> None:
    parser = argparse.ArgumentParser(description="Fetch satellite imagery from GEE")
    parser.add_argument("--year", type=int, help="Override: fetch single year only")
    parser.add_argument("--sensor", choices=["s2", "l8"], default="s2")
    parser.add_argument("--drive", action="store_true", help="Export to Drive instead of download")
    args = parser.parse_args()

    authenticate_gee()
    roi = get_roi()

    if args.year:
        season = T0_SEASON if args.year <= T0_YEAR else T1_SEASON
        if args.sensor == "s2":
            composite = build_s2_composite(roi, args.year, season)
        else:
            composite = build_l8_composite(roi, args.year, season)

        if composite and args.drive:
            export_to_drive(composite, roi, f"leila_{args.sensor}_{args.year}")
        elif composite:
            download_small_region(composite, roi, f"{args.sensor}_{args.year}.tif")
    else:
        run()


if __name__ == "__main__":
    main()
