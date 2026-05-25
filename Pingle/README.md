# Biodiversity Delta Engine (BDE)

Automated spatial analytics pipeline calculating the **Statutory Biodiversity Metric 4.0** delta between 2016 baseline and 2026 present day for a 1km radius study area around Pingle, Taylor's Lane, Ashleyhay, Derbyshire (DE4 4AH).

## What It Does

- Ingests Sentinel-2 and Landsat 8 satellite imagery via Google Earth Engine
- Classifies habitats to UKHab Level 3 using Random Forest
- Computes full BM4.0 biodiversity units with Monte Carlo uncertainty propagation
- Cross-references OS MasterMap, Natural England PHI, SSSI/SAC/SPA designations, and HM Land Registry cadastral parcels
- Mines university thesis repositories for local ecological research data
- Verifies all citations with URL checking and SHA-256 content hashing
- Generates a ~200-page LaTeX report with figures, aerial imagery, and BibTeX bibliography

## Architecture

```
src/
├── config.py            # Central configuration (centroid, CRS, BM4.0 tables)
├── bbox.py              # BNG bounding box generation
├── gee_ingestion.py     # Google Earth Engine satellite composites
├── os_data_hub.py       # Ordnance Survey MasterMap + Places API
├── magic_map.py         # Natural England PHI, SSSI, SAC, SPA
├── land_registry.py     # HM Land Registry INSPIRE cadastral
├── habitat_classifier.py # Random Forest UKHab L3 classifier
├── bm4_calculator.py    # Full BM4.0 engine + Monte Carlo
├── figure_generator.py  # 9 figure types (PNG + PDF)
├── aerial_downloader.py # Google Maps Static API tile stitching
├── thesis_miner.py      # 8 university repository searches
├── citation_verifier.py # BibTeX URL verification + SHA-256
├── spot_checker.py      # DDDC planning portal search
└── pipeline.py          # 4-phase master orchestrator

latex/
├── main.tex             # Master document (DreamLab AI branded)
├── chapters/            # 11 chapters + appendices
├── frontmatter/         # Title page, abstract, acknowledgements
└── bib/references.bib   # 22+ verified BibTeX entries

config/                  # API keys and credentials (gitignored)
```

## Setup

```bash
python -m venv .venv
source .venv/bin/activate
pip install earthengine-api geopandas rasterio scikit-learn shapely pyproj requests beautifulsoup4 bibtexparser matplotlib
```

Set environment variables:
```bash
export GOOGLE_APPLICATION_CREDENTIALS=config/ee-jjohare-sa-key.json
export OS_DATA_HUB_API_KEY=<your-key>
export PERPLEXITY_API_KEY=<your-key>
export GOOGLE_MAPS_API_KEY=<your-key>
```

## Run

```bash
python src/pipeline.py
```

## Engineering Specification

See [BDE-PRD-ADR-DDD.md](BDE-PRD-ADR-DDD.md) for the full engineering spec including:
- PRD-BDE-001: 11 functional requirements, 5 non-functional requirements, risk register
- ADR-BDE-001..004: Swarm topology, spatial stack, BM4.0 formula, citation verification
- DDD-BDE-001: 3 bounded contexts, aggregate roots, domain events, ubiquitous language

All claims carry confidence annotations: **[HIGH]** / **[MEDIUM]** / **[LOW]** / **[SPECULATIVE]**.

## Coordinate Reference System

All spatial operations use **EPSG:27700** (British National Grid).
Study area centroid: BNG (428500, 353000) / WGS84 (53.0694, -1.5456).

## License

Copyright 2026 DreamLab AI. All rights reserved.
