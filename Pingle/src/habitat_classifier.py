#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — UKHab Habitat Classifier

Random Forest classifier using Sentinel-2 spectral bands + NDVI + NDWI,
trained on Natural England Priority Habitat Inventory ground truth.
Classifies to UKHab Level 3 and outputs both raster and vector products.

CONFIDENCE: MEDIUM
- Satellite-based habitat classification typically achieves 75-85% accuracy
  for UKHab Level 3 in mixed rural landscapes (Sheridan et al. 2020,
  Medcalf et al. 2014).
- Main uncertainty sources:
  * Spectral confusion between grassland subtypes (modified vs neutral)
  * Woodland understory invisible to satellite
  * Condition assessment cannot be derived from spectral data alone
  * Training data (PHI) has its own positional and thematic errors
- This is the primary uncertainty driver in the entire BNG calculation chain.
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
import rasterio
from rasterio.features import rasterize, shapes
from rasterio.mask import mask as rio_mask
from sklearn.ensemble import RandomForestClassifier
from sklearn.metrics import (
    accuracy_score,
    classification_report,
    confusion_matrix,
)
from sklearn.model_selection import StratifiedKFold, cross_val_score

from config import (
    CRS_BNG,
    DATA_DIR,
    FIGURES_DIR,
    RF_MAX_DEPTH,
    RF_N_ESTIMATORS,
    RF_RANDOM_STATE,
    setup_logging,
)

log = setup_logging("habitat_classifier")

# ---------------------------------------------------------------------------
# UKHab Level 3 class mapping
# ---------------------------------------------------------------------------

# Map from PHI Main_Habit / descriptiveGroup to UKHab class codes
# CONFIDENCE: MEDIUM — PHI habitat names don't map 1:1 to UKHab;
# crosswalk based on Natural England UKHab Correspondence Table v1.1
UKHAB_MAPPING = {
    # PHI habitat name → UKHab code and label
    "Lowland Meadows": ("g3a", "Lowland meadow"),
    "Lowland Calcareous Grassland": ("g1a", "Lowland calcareous grassland"),
    "Upland Acid Grassland": ("g1c", "Upland acid grassland"),
    "Lowland Dry Acid Grassland": ("g1b", "Lowland dry acid grassland"),
    "Purple Moor Grass and Rush Pastures": ("g3b", "Purple moor grass"),
    "Coastal and Floodplain Grazing Marsh": ("g3c", "Floodplain grazing marsh"),
    "Deciduous Woodland": ("w1f", "Broadleaved woodland"),
    "Lowland Mixed Deciduous Woodland": ("w1f", "Broadleaved woodland"),
    "Upland Mixed Ashwoods": ("w1f", "Broadleaved woodland"),
    "Wet Woodland": ("w1g", "Wet woodland"),
    "Wood-Pasture and Parkland": ("w1h", "Wood-pasture"),
    "Traditional Orchard": ("w1e", "Traditional orchard"),
    "Lowland Heathland": ("h1a", "Lowland heathland"),
    "Upland Heathland": ("h1b", "Upland heathland"),
    "Blanket Bog": ("f1", "Blanket bog"),
    "Lowland Raised Bog": ("f1", "Lowland raised bog"),
    "Lowland Fens": ("f2a", "Lowland fens"),
    "Reedbeds": ("f2b", "Reedbeds"),
    "Ponds": ("r1b", "Pond"),
    "Rivers": ("r2a", "Running water"),
}

# Integer encoding for classifier
# CONFIDENCE: HIGH — deterministic encoding
CLASS_LABELS = {
    "Cropland": 1,
    "Modified grassland": 2,
    "Other neutral grassland": 3,
    "Lowland meadow": 4,
    "Lowland calcareous grassland": 5,
    "Broadleaved woodland": 6,
    "Coniferous woodland": 7,
    "Mixed woodland": 8,
    "Mixed scrub": 9,
    "Heathland": 10,
    "Developed land": 11,
    "Bare ground": 12,
    "Standing water": 13,
    "Running water": 14,
    "Traditional orchard": 15,
}
CLASS_NAMES = {v: k for k, v in CLASS_LABELS.items()}


# ---------------------------------------------------------------------------
# Training data preparation
# ---------------------------------------------------------------------------

def prepare_training_data(
    raster_path: Path,
    phi_path: Path,
    os_topo_path: Optional[Path] = None,
) -> tuple[np.ndarray, np.ndarray, list[str]]:
    """
    Extract spectral features and labels from raster using PHI polygons.

    CONFIDENCE: MEDIUM
    - PHI polygons serve as training labels (assumed correct)
    - Pixels fully within PHI polygons are sampled
    - Mixed pixels at polygon edges introduce noise
    - OS Topography used to identify developed land / sealed surfaces
    """
    with rasterio.open(raster_path) as src:
        band_names = [src.descriptions[i] if src.descriptions[i] else f"band_{i+1}"
                      for i in range(src.count)]
        raster_data = src.read()  # shape: (bands, rows, cols)
        transform = src.transform
        raster_shape = (src.height, src.width)
        raster_crs = src.crs

    log.info("Raster: %d bands, %dx%d pixels", src.count, src.width, src.height)

    # Load PHI training polygons
    phi = gpd.read_file(phi_path)
    if phi.crs != raster_crs:
        phi = phi.to_crs(raster_crs)

    # Map PHI habitats to integer classes
    # CONFIDENCE: MEDIUM — mapping completeness depends on habitats present
    phi["class_id"] = 0
    for phi_name, (ukhab_code, ukhab_label) in UKHAB_MAPPING.items():
        if "Main_Habit" in phi.columns:
            mask = phi["Main_Habit"].str.contains(phi_name, case=False, na=False)
            if mask.any() and ukhab_label in CLASS_LABELS:
                phi.loc[mask, "class_id"] = CLASS_LABELS[ukhab_label]

    # Drop unmapped
    phi = phi[phi["class_id"] > 0]
    log.info("Training polygons with valid class: %d", len(phi))

    if len(phi) == 0:
        log.error("No training polygons mapped to UKHab classes")
        raise ValueError("No valid training data")

    # Optionally add OS Topography for developed land
    if os_topo_path and os_topo_path.exists():
        os_topo = gpd.read_file(os_topo_path)
        if os_topo.crs != raster_crs:
            os_topo = os_topo.to_crs(raster_crs)

        # Identify built-up areas from descriptiveGroup
        if "descriptiveGroup" in os_topo.columns:
            built = os_topo[
                os_topo["descriptiveGroup"].str.contains(
                    "Building|Structure|Road", case=False, na=False
                )
            ].copy()
            if len(built) > 0:
                built["class_id"] = CLASS_LABELS["Developed land"]
                phi = pd.concat(
                    [phi[["geometry", "class_id"]], built[["geometry", "class_id"]]],
                    ignore_index=True,
                )
                log.info("Added %d OS Topo built-up polygons as training data", len(built))

    # Rasterize training labels
    label_raster = rasterize(
        [(geom, class_id) for geom, class_id in zip(phi.geometry, phi["class_id"])],
        out_shape=raster_shape,
        transform=transform,
        fill=0,
        dtype=np.int32,
    )

    # Extract pixels where we have labels
    valid = label_raster > 0
    n_valid = valid.sum()
    log.info("Labelled pixels: %d (%.1f%% of raster)", n_valid, 100 * n_valid / label_raster.size)

    if n_valid < 50:
        log.error("Insufficient labelled pixels (%d) — need at least 50", n_valid)
        raise ValueError(f"Only {n_valid} labelled pixels available")

    X = raster_data[:, valid].T  # shape: (n_pixels, n_bands)
    y = label_raster[valid]       # shape: (n_pixels,)

    # Remove any NaN/nodata pixels
    finite_mask = np.all(np.isfinite(X), axis=1)
    X = X[finite_mask]
    y = y[finite_mask]

    log.info(
        "Training data: %d samples, %d features, %d classes",
        X.shape[0], X.shape[1], len(np.unique(y)),
    )

    return X, y, band_names


# ---------------------------------------------------------------------------
# Classifier training and prediction
# ---------------------------------------------------------------------------

def train_classifier(
    X: np.ndarray,
    y: np.ndarray,
    n_estimators: int = RF_N_ESTIMATORS,
    max_depth: int = RF_MAX_DEPTH,
) -> tuple[RandomForestClassifier, dict]:
    """
    Train a Random Forest classifier with cross-validation assessment.

    CONFIDENCE: MEDIUM
    - RF is robust for medium-sized training sets
    - Cross-validation gives realistic accuracy estimate
    - Feature importance indicates which bands drive classification
    - Stratified K-fold ensures class balance in folds
    """
    log.info("Training Random Forest: n_estimators=%d, max_depth=%d", n_estimators, max_depth)

    clf = RandomForestClassifier(
        n_estimators=n_estimators,
        max_depth=max_depth,
        random_state=RF_RANDOM_STATE,
        n_jobs=-1,
        class_weight="balanced",  # Handle class imbalance
    )

    # Cross-validation
    # CONFIDENCE: HIGH for methodology (stratified 5-fold is standard)
    cv = StratifiedKFold(n_splits=5, shuffle=True, random_state=RF_RANDOM_STATE)
    cv_scores = cross_val_score(clf, X, y, cv=cv, scoring="accuracy")
    log.info(
        "Cross-validation accuracy: %.3f +/- %.3f",
        cv_scores.mean(), cv_scores.std(),
    )

    # Train final model on all data
    clf.fit(X, y)

    # Metrics on training set (for reference — CV score is more reliable)
    y_pred = clf.predict(X)
    train_acc = accuracy_score(y, y_pred)
    cm = confusion_matrix(y, y_pred)

    present_classes = np.unique(y)
    target_names = [CLASS_NAMES.get(c, f"class_{c}") for c in present_classes]
    report = classification_report(
        y, y_pred,
        target_names=target_names,
        output_dict=True,
    )

    metrics = {
        "cv_accuracy_mean": float(cv_scores.mean()),
        "cv_accuracy_std": float(cv_scores.std()),
        "train_accuracy": float(train_acc),
        "n_samples": int(len(y)),
        "n_classes": int(len(present_classes)),
        "classes": {int(c): CLASS_NAMES.get(c, f"class_{c}") for c in present_classes},
        "feature_importance": dict(
            zip(range(X.shape[1]), clf.feature_importances_.tolist())
        ),
        "confusion_matrix": cm.tolist(),
        "classification_report": report,
    }

    log.info("Training accuracy: %.3f (use CV score for unbiased estimate)", train_acc)

    return clf, metrics


def classify_raster(
    clf: RandomForestClassifier,
    raster_path: Path,
    output_path: Path,
) -> Path:
    """
    Apply trained classifier to entire raster, output classified GeoTIFF.

    CONFIDENCE: MEDIUM — classification quality depends on training quality.
    """
    with rasterio.open(raster_path) as src:
        raster_data = src.read()  # (bands, rows, cols)
        profile = src.profile.copy()
        nodata_mask = np.any(~np.isfinite(raster_data), axis=0)

    n_bands, rows, cols = raster_data.shape
    flat = raster_data.reshape(n_bands, -1).T  # (pixels, bands)

    # Handle nodata
    finite = np.all(np.isfinite(flat), axis=1)
    predictions = np.zeros(flat.shape[0], dtype=np.int32)
    if finite.sum() > 0:
        predictions[finite] = clf.predict(flat[finite])

    classified = predictions.reshape(rows, cols)
    classified[nodata_mask] = 0  # nodata

    # Write output
    profile.update(
        count=1,
        dtype="int32",
        nodata=0,
    )
    with rasterio.open(output_path, "w", **profile) as dst:
        dst.write(classified, 1)

    log.info("Classified raster written: %s", output_path)
    return output_path


def vectorise_classification(
    classified_path: Path,
    output_path: Path,
) -> gpd.GeoDataFrame:
    """
    Convert classified raster to vector polygons with UKHab labels.

    CONFIDENCE: HIGH for methodology (standard raster-to-vector).
    """
    with rasterio.open(classified_path) as src:
        classified = src.read(1)
        transform = src.transform
        crs = src.crs

    # Generate polygons from raster
    results = []
    for geom, value in shapes(classified, transform=transform):
        if value > 0:
            results.append({
                "geometry": geom,
                "class_id": int(value),
                "ukhab_label": CLASS_NAMES.get(int(value), f"unknown_{value}"),
            })

    if not results:
        log.warning("No classified polygons generated")
        return gpd.GeoDataFrame()

    from shapely.geometry import shape
    gdf = gpd.GeoDataFrame(results, crs=crs)
    gdf["geometry"] = gdf["geometry"].apply(shape)

    # Calculate areas
    gdf["area_m2"] = gdf.geometry.area
    gdf["area_ha"] = gdf["area_m2"] / 10000.0

    # Summary
    summary = gdf.groupby("ukhab_label")["area_ha"].sum().sort_values(ascending=False)
    log.info("Classification summary:")
    for hab, area in summary.items():
        log.info("  %s: %.2f ha", hab, area)

    gdf.to_file(output_path, driver="GeoJSON")
    log.info("Vectorised classification: %s (%d polygons)", output_path, len(gdf))

    return gdf


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run(
    raster_path: Optional[Path] = None,
    phi_path: Optional[Path] = None,
    os_topo_path: Optional[Path] = None,
    label: str = "t1",
) -> tuple[Optional[gpd.GeoDataFrame], dict]:
    """
    Execute the full habitat classification pipeline for one time period.

    Returns (classified_vector_gdf, metrics_dict).
    """
    if raster_path is None:
        # Default paths from GEE ingestion
        raster_path = DATA_DIR / f"{label}_s2_composite.tif"
    if phi_path is None:
        phi_path = DATA_DIR / "ne_priority_habitats.geojson"
    if os_topo_path is None:
        os_topo_path = DATA_DIR / "os_topography.geojson"

    if not raster_path.exists():
        log.error("Raster not found: %s", raster_path)
        return None, {}
    if not phi_path.exists():
        log.error("PHI training data not found: %s", phi_path)
        return None, {}

    # Prepare training data
    topo = os_topo_path if os_topo_path.exists() else None
    X, y, band_names = prepare_training_data(raster_path, phi_path, topo)

    # Train classifier
    clf, metrics = train_classifier(X, y)

    # Classify full raster
    classified_raster = DATA_DIR / f"{label}_classified.tif"
    classify_raster(clf, raster_path, classified_raster)

    # Vectorise
    classified_vector = DATA_DIR / f"{label}_classified.geojson"
    gdf = vectorise_classification(classified_raster, classified_vector)

    # Save metrics
    metrics_path = DATA_DIR / f"{label}_classification_metrics.json"
    with open(metrics_path, "w") as f:
        json.dump(metrics, f, indent=2)
    log.info("Metrics saved: %s", metrics_path)

    return gdf, metrics


def main() -> None:
    parser = argparse.ArgumentParser(description="UKHab habitat classification")
    parser.add_argument("--raster", type=Path, help="Input composite GeoTIFF")
    parser.add_argument("--phi", type=Path, help="PHI training polygons GeoJSON")
    parser.add_argument("--os-topo", type=Path, help="OS Topography GeoJSON")
    parser.add_argument("--label", default="t1", help="Time period label (t0/t1)")
    args = parser.parse_args()

    run(
        raster_path=args.raster,
        phi_path=args.phi,
        os_topo_path=args.os_topo,
        label=args.label,
    )


if __name__ == "__main__":
    main()
