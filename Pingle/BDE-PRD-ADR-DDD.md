# Biodiversity Delta Engine (BDE) — Integrated Engineering Blueprint

**PRD-BDE-001 | ADR-BDE-001..004 | DDD-BDE-001**

| Field | Value |
|---|---|
| Author | Dr John O'Hare, DreamLab AI |
| Status | Draft |
| Created | 2025-05-25 |
| Target Site | Pingle, Taylor's Lane, Ashleyhay, Derbyshire |
| Postcode Centroid | DE4 4AH (53.0694°N, 1.5456°W) |
| Bounding Box | 1 km radial buffer → BBOX ~SK2853 (BNG) |
| Temporal Baseline T₀ | 2016 calendar year |
| Temporal Present T₁ | 2026 calendar year |
| Output | 200-page LaTeX PDF with Python mathematical modelling |

---

## Part 1 — Product Requirements Document (PRD-BDE-001)

### 1.1 Problem Statement

UK planning law (Environment Act 2021, mandatory from Feb 2024) requires a minimum 10% Biodiversity Net Gain (BNG) on all major developments. **[CONFIDENCE: HIGH]** — primary legislation, unambiguous. The **Statutory Biodiversity Metric 4.0** (DEFRA, 2023) is the mandated calculation framework. **[CONFIDENCE: HIGH]** — published DEFRA specification, version confirmed January 2024. Calculating the metric requires:

1. A verified **baseline habitat survey** (T₀)
2. A current **habitat condition assessment** (T₁)
3. Quantified **biodiversity unit deltas** across Area, Hedgerow, and Watercourse modules
4. Evidence of habitat distinctiveness, condition, strategic significance, and connectivity

Current practice relies on manual ecologist field surveys costing £15k–£50k per site, with a 6–12 week turnaround. **[CONFIDENCE: MEDIUM]** — cost range sourced from industry estimates and planning consultant fee schedules; actual costs vary significantly by site complexity and ecologist availability. This project automates the spatial analysis pipeline and cross-validates against published ecologist data to produce audit-grade outputs.

### 1.2 Success Criteria

| ID | Criterion | Measure | Confidence |
|---|---|---|---|
| SC-1 | Habitat classification accuracy vs published ecologist reports | ≥85% agreement on habitat type at UKHab Level 3 | **[MEDIUM]** — 75–85% is typical for RF classification on 10m Sentinel-2 in mixed rural landscapes; 85% is aspirational and depends on training data quality and habitat heterogeneity within the BBOX |
| SC-2 | BU delta within tolerance of manual calculation | ±5% of hand-calculated reference | **[MEDIUM]** — formula implementation is deterministic, but condition scoring from remote sensing introduces systematic uncertainty that may exceed ±5% |
| SC-3 | All citations machine-verified | 0 dead links, 0 fabricated references | **[HIGH]** — straightforward HTTP verification; only risk is ephemeral URLs changing between verification and publication |
| SC-4 | Report compiles to PDF without manual intervention | `latexmk -pdf` exits 0 | **[HIGH]** — standard toolchain, deterministic compilation |
| SC-5 | Spot-check validation against ≥3 local published BNG assessments | Cross-examination report per site | **[MEDIUM]** — Derbyshire Dales is an active planning area but availability of published BNG assessments with sufficient detail for parcel-level comparison is uncertain |

### 1.3 Data Ingestion Requirements

#### 1.3.1 Spatial Imagery (Raster)

| Source | API/Method | T₀ (2016) | T₁ (2026) | CRS | Confidence |
|---|---|---|---|---|---|
| Google Earth Engine | `ee.ImageCollection('COPERNICUS/S2')` | Sentinel-2 L2A (launch July 2015, first full year 2016) | Current composite | EPSG:32630 → reproject EPSG:27700 | **[MEDIUM]** — Sentinel-2A was operational from June 2015 but UK coverage wasn't complete or systematic until mid-2016. The growing season window Apr–Sep 2016 may have significant cloud-cover gaps over DE4 4AH. Actual availability unknown until GEE query executed. Sentinel-2B launched March 2017, so 2016 has single-satellite revisit cadence (~10 days vs ~5 days post-2017). |
| Google Earth Engine | `ee.ImageCollection('LANDSAT/LC08/C02/T1_L2')` | Landsat 8 OLI 30m | Current | EPSG:32630 → EPSG:27700 | **[HIGH]** — Landsat 8 operational since Feb 2013, well-established 16-day revisit; 2016 growing season imagery reliably available. 30m resolution is a constraint for small-parcel classification but data existence is not in doubt. |
| Environment Agency LIDAR | DEFRA Data Services Platform REST API | 1m DTM/DSM (2016 flight if available, else nearest) | Latest flight | EPSG:27700 native | **[MEDIUM]** — EA LIDAR coverage is extensive but not nationwide for any single year. Whether a 2016 flight covers the DE4 4AH BBOX specifically is unknown until queried. Nearest-year fallback (2015 or 2017) is highly likely. |
| Google Maps Static/Tiles API | Maps Platform | Historical streetview context | Current aerial tiles | WGS84 → EPSG:27700 | **[HIGH]** — well-documented commercial API with stable availability. |

#### 1.3.2 Vector / Topographic

| Source | API/Method | Purpose | Confidence |
|---|---|---|---|
| OS Data Hub — Features API | OAuth2, `wfs:Topography_TopographicArea` | MasterMap polygons, land use boundaries | **[HIGH]** — well-documented, stable API, BNG native. OS Data Hub has been reliable since 2020 launch. |
| OS Data Hub — Places API | Postcodes/UPRN lookup | Centroid resolution for DE4 4AH | **[HIGH]** — standard postcode lookup, deterministic. |
| MAGIC Map (Natural England) | WMS/WFS `https://magic.defra.gov.uk/` | SSSI, SAC, SPA, priority habitat overlays | **[HIGH]** for current designations — stable WMS service operational for >15 years. **[MEDIUM]** for historic (2016) designation boundaries — MAGIC does not version-control historic snapshots in a user-accessible way. WMS TIME parameter support is inconsistent across layers. |
| Natural England Open Data | ArcGIS REST `PHI_v2_3` | Priority Habitat Inventory (2016 snapshot + current) | **[MEDIUM]** — the PHI is periodically updated but historic snapshots are not versioned in their public API. The current PHI (v2.3) reflects the latest survey data, not a 2016 point-in-time. Reconstructing 2016 state may require cross-referencing with archived classification or using the 2016 Sentinel-2 classification as a proxy. |
| HM Land Registry | INSPIRE Index Polygons (OGC WFS) | Cadastral boundaries — parcel delineation | **[HIGH]** — open WFS, no authentication required, stable schema. INSPIRE polygons cover England and Wales comprehensively. |
| National Biodiversity Network (NBN) Atlas | NBN Atlas API | Species occurrence records within BBOX | **[HIGH]** for API availability. **[MEDIUM]** for data density — species records for a specific 1km BBOX in rural Derbyshire may be sparse depending on recording effort. |

#### 1.3.3 Academic Research — Local University Theses & Dissertations (2014–2018)

The Derbyshire Dales / White Peak / Derwent Valley corridor is well-studied. **[CONFIDENCE: MEDIUM]** — the corridor is a known research landscape but whether any thesis contains fieldwork data precisely overlapping our 1km BBOX is unknown. The following institutional repositories must be searched by the Verification Mesh for Masters and PhD theses containing ground-truth ecological survey data, species inventories, or habitat condition assessments overlapping the BBOX or surrounding landscape:

| Institution | Repository | Search Endpoint | Relevance | Confidence |
|---|---|---|---|---|
| **University of Derby** | UDORA (Derby Online Research Archive) | `https://derby.openrepository.com/` | Geography, Environmental Science, Conservation Biology programmes. Derby is 15 km from target — high likelihood of local fieldwork theses | **[MEDIUM]** — repository exists and is searchable, but search API capabilities (structured query support, date filtering) vary. UDORA uses DSpace which supports OAI-PMH. |
| **University of Nottingham** | Nottingham eTheses | `https://eprints.nottingham.ac.uk/` | School of Geography, School of Biosciences. Strong ecology/remote sensing research. Sherwood–Peak District corridor studies | **[MEDIUM]** — EPrints platform, well-structured. Relevance to our specific BBOX is speculative. |
| **University of Sheffield** | White Rose eTheses Online (WREO) | `https://etheses.whiterose.ac.uk/` | Dept of Animal & Plant Sciences, Dept of Geography. Peak District is their primary fieldwork landscape | **[MEDIUM]** — highest likelihood of relevant Peak District ecology theses. White Rose consortium is well-maintained. |
| **Sheffield Hallam University** | SHURA | `https://shura.shu.ac.uk/` | Environment & Geography. Applied ecology, habitat management research | **[LOW]** — smaller research output; repository search API less mature. |
| **Nottingham Trent University** | IRep | `https://irep.ntu.ac.uk/` | School of Animal, Rural & Environmental Sciences. Applied biodiversity, land management | **[LOW]** — relevant school but repository search granularity is limited. |
| **University of Leicester** | Leicester Research Archive | `https://figshare.le.ac.uk/` | Geography, Geology & Environment. East Midlands landscape ecology | **[LOW]** — Figshare-based, good API but Leicester is further from the study area. |
| **Keele University** | Keele Research Repository | `https://eprints.keele.ac.uk/` | School of Life Sciences. North Midlands ecology, SSSIs | **[LOW]** — smaller repository, limited ecology output. |
| **University of Birmingham** | eTheses Repository | `https://etheses.bham.ac.uk/` | School of Geography, Earth and Environmental Sciences. National-scale BNG research, Midlands case studies | **[MEDIUM]** — strong geography department but study area is at the edge of their typical fieldwork range. |

**Search queries per repository:**
```
"biodiversity" AND ("Derbyshire" OR "Peak District" OR "Derwent Valley" OR "White Peak")
"habitat survey" AND ("Derbyshire Dales" OR "Ashleyhay" OR "Wirksworth" OR "Ambergate")
"land cover classification" AND ("Derbyshire" OR "East Midlands")
"hedgerow" AND ("Derbyshire" OR "Peak District")
"SSSI condition" AND ("Derbyshire" OR "Peak District")
```

**[CONFIDENCE: MEDIUM]** — queries are well-constructed for recall but actual search API support for Boolean queries varies by repository platform (DSpace, EPrints, Figshare each handle Boolean differently). Some may require simplified keyword search.

**Temporal filter:** 2014–2018 (captures research conducted around the 2016 baseline, including theses submitted 1–2 years after fieldwork).

**Extraction targets:**
- Raw species lists or habitat inventories with grid references overlapping BBOX
- Vegetation survey quadrat data (NVC or UKHab classified)
- Condition assessment scores for local SSSIs or priority habitats
- Remote sensing classification accuracy assessments for the region
- Soil, hydrology, or microclimate data relevant to habitat condition scoring

**Integration:** Thesis data feeds into the Spot-Check Cross-Examination (FR-08) as additional ground-truth calibration points beyond published planning portal BNG assessments. Where a thesis provides per-quadrat vegetation data within the BBOX, it becomes a high-confidence reference for the Random Forest classifier training/validation split. **[CONFIDENCE: LOW]** — the probability of finding thesis quadrat data precisely within our 1km BBOX is low; more likely we will find landscape-scale data for the wider Derbyshire Dales that provides contextual calibration rather than direct ground truth.

#### 1.3.4 Published BNG Assessments (Spot-Check Corpus)

| Source | Method | Purpose | Confidence |
|---|---|---|---|
| Derbyshire Dales District Council Planning Portal | Perplexity API scrape + direct URL fetch | Extract published Biodiversity Metric calculations from planning applications within 5 km | **[LOW]** — planning portal scraping is brittle and varies by council portal technology. Derbyshire Dales uses the Idox Uniform portal; ecology reports may be PDFs, scanned documents, or HTML. Structured data extraction reliability is low. |
| DEFRA Biodiversity Gain Site Register | GOV.UK API | Registered gain sites near target | **[HIGH]** — published government register with structured API. However, the register is new (2024) so the number of registered sites near DE4 4AH may be very small. |
| Local Nature Recovery Strategy (Derbyshire) | PDF extraction | Strategic significance multiplier validation | **[HIGH]** — published by Derbyshire County Council. PDF is publicly available. Strategic significance zones are mapped. |

### 1.4 Functional Requirements

| ID | Requirement | Acceptance Criteria | Confidence |
|---|---|---|---|
| FR-01 | **Centroid Resolution** — Resolve DE4 4AH to BNG easting/northing, generate 1 km buffer polygon | Output: GeoJSON polygon in EPSG:27700, verified against OS Places API | **[HIGH]** — deterministic geocoding operation. |
| FR-02 | **Raster Ingestion** — Fetch cloud-free Sentinel-2 composites for T₀ (2016 growing season Apr–Sep) and T₁ (2026 growing season) via GEE | Output: Two GeoTIFF stacks (RGB + NDVI + NDWI bands) clipped to BBOX | **[MEDIUM]** — T₁ composite is HIGH confidence. T₀ depends on 2016 cloud cover over DE4 4AH which is unknown until query time. |
| FR-03 | **Habitat Classification** — Segment raster into UKHab Level 3 categories using supervised classification (Random Forest trained on Natural England ground truth) | Output: Classified raster + vector polygons with UKHab codes | **[MEDIUM]** — 75–85% accuracy typical for Level 3 classification in mixed rural landscapes from 10m Sentinel-2. Main uncertainty source for the entire pipeline. |
| FR-04 | **Vector Overlay** — Intersect classified polygons with OS MasterMap, Land Registry parcels, and MAGIC designations | Output: Attributed GeoPackage with per-parcel habitat type, area (ha), designation flags | **[HIGH]** — standard GIS intersection operations with well-defined inputs. |
| FR-05 | **BM4.0 Calculation Engine** — Compute biodiversity units per parcel using the full statutory formula | Output: DataFrame with columns: parcel_id, habitat_type, area_ha, distinctiveness, condition, strategic_significance, connectivity, temporal_multiplier, spatial_risk, difficulty, BU_T0, BU_T1, delta_BU | **[HIGH]** for formula correctness (directly from DEFRA published specification). **[LOW]** for condition input accuracy (see FR-03 and section 1.9). |
| FR-06 | **Hedgerow Module** — Detect linear hedgerow features from LIDAR canopy height model, classify condition | Output: Hedgerow units (length × distinctiveness × condition) | **[MEDIUM]** — hedgerow detection from LIDAR CHM is well-established but condition classification from remote sensing alone is experimental. Length measurement is HIGH confidence; condition scoring is LOW. |
| FR-07 | **Watercourse Module** — Extract watercourse features from OS VectorMap + EA flood risk layers | Output: Watercourse/river units | **[HIGH]** — watercourse geometry from OS VectorMap is definitive. Encroachment and riparian condition assessment from satellite is LOW confidence. |
| FR-08 | **Spot-Check Cross-Examination** — For each published BNG assessment within 5 km, compare ecologist's habitat classification vs AI classification for the same parcels | Output: Per-site comparison matrix with agreement %, flagged discrepancies | **[MEDIUM]** — dependent on finding published assessments with sufficient parcel-level detail. See section 1.3.4 confidence notes. |
| FR-09 | **Academic Thesis Mining** — Search 8 local university repositories (2014–2018) for ecology/biodiversity theses with fieldwork data overlapping or near the BBOX | Output: Thesis index with extracted quadrat data, species lists, habitat classifications; integrated as ground-truth calibration for RF classifier | **[MEDIUM]** for search execution. **[LOW]** for finding data precisely within the BBOX. See section 1.3.3 confidence notes. |
| FR-10 | **Citation Verification** — Every factual claim, data source, and reference in the final report verified by agentic swarm | Output: Citation log with URL, access date, HTTP status, content hash | **[HIGH]** — mechanical verification process. |
| FR-11 | **LaTeX Report Generation** — Compile all outputs into structured 200-page PDF | Output: `bde-report.pdf` via `latexmk` | **[HIGH]** for compilation. **[MEDIUM]** for content completeness — the 200-page target depends on all upstream data sources yielding usable results. If key data sources fail (e.g., 2016 Sentinel-2 cloud cover is total), entire chapters may need to document the gap rather than present analysis. |

### 1.5 Non-Functional Requirements

| ID | Requirement | Target | Confidence |
|---|---|---|---|
| NFR-01 | Full pipeline execution time | < 4 hours wall clock | **[MEDIUM]** — depends on GEE export queue times and Perplexity API rate limits. 4 hours is achievable under normal load but GEE exports can queue for 30+ minutes during peak. |
| NFR-02 | Spatial accuracy | Sub-10m positional (Sentinel-2 native resolution) | **[HIGH]** — inherent to Sentinel-2 L2A geometric accuracy specification. |
| NFR-03 | Reproducibility | Deterministic given same input imagery dates; seeded RNG | **[HIGH]** — achievable with explicit random seeds and pinned dependency versions. |
| NFR-04 | Auditability | Every intermediate artefact persisted with provenance metadata | **[HIGH]** — design decision, not dependent on external factors. |
| NFR-05 | Statutory alignment | BM4.0 (Jan 2024 release) — not BM3.1 or earlier | **[HIGH]** — DEFRA specification is published and version-controlled. |

### 1.6 Report Structure (LaTeX Chapter Plan)

| Ch | Title | Est. Pages | Content | Confidence |
|---|---|---|---|---|
| 1 | Executive Summary | 5 | Key findings, net BU delta, compliance statement | **[HIGH]** |
| 2 | Methodology | 15 | Data sources, classification pipeline, BM4.0 formula derivation | **[HIGH]** |
| 3 | Site Context | 10 | Location, designations, planning history, geology, hydrology | **[HIGH]** — factual compilation from open sources |
| 4 | Baseline Assessment (T₀ 2016) | 30 | Per-parcel habitat maps, distinctiveness scores, condition ratings | **[MEDIUM]** — page count depends on 2016 data availability and classification quality |
| 5 | Current Assessment (T₁ 2026) | 30 | Same structure as Ch 4 for present day | **[HIGH]** — 2026 data sources are all well-established |
| 6 | Delta Analysis | 25 | Change detection maps, BU calculations, sensitivity analysis | **[MEDIUM]** — sensitivity analysis may expand if uncertainty is high |
| 7 | Hedgerow & Watercourse Modules | 15 | Linear feature analysis | **[MEDIUM]** — depends on LIDAR availability for 2016 |
| 8 | Spot-Check Validation | 20 | Cross-examination against published ecologist reports | **[LOW]** — page count is highly dependent on number of published assessments found within 5km |
| 9 | Mathematical Modelling | 20 | Python model documentation, Monte Carlo uncertainty, spatial autocorrelation | **[HIGH]** — model documentation is under our control |
| 10 | Charts & Figures | 15 | All matplotlib/geopandas outputs with captions | **[HIGH]** |
| 11 | Citations & References | 5 | BibTeX bibliography, verified URLs | **[HIGH]** |
| A | Appendix: Raw Data Tables | 10 | Full parcel-level data frames | **[HIGH]** |
| B | Appendix: Agent Verification Logs | 5 | Swarm consensus records, reflow traces | **[HIGH]** |
| — | **Total** | **~205** | — | **[MEDIUM]** — total is plausible but individual chapter sizes will flex based on data availability |

### 1.7 Risk Register

| Risk | Impact | Likelihood | Mitigation | Confidence in Mitigation |
|---|---|---|---|---|
| 2016 Sentinel-2 imagery cloud-contaminated for target BBOX | Baseline accuracy degraded | Medium (UK weather) | Fall back to Landsat 8 30m; flag reduced resolution in report | **[HIGH]** — Landsat 8 fallback is reliable. Resolution degradation (30m vs 10m) affects small-parcel classification but is manageable. |
| Natural England PHI 2016 snapshot not available as discrete layer | Cannot diff T₀ vs T₁ | Medium | Use MAGIC Map WMS time parameter; if unavailable, reconstruct from 2016 aerial classification | **[MEDIUM]** — WMS TIME parameter support is inconsistent. Reconstruction from classification introduces circular dependency with the RF classifier. |
| Published BNG assessments not available for nearby sites | Cannot cross-validate | Low (active planning area) | Expand search radius to 10 km; use EA habitat surveys as fallback | **[MEDIUM]** — 10km expansion increases likelihood but BNG mandatory assessments only started Feb 2024; the corpus is young. |
| UKHab classification from satellite disagrees with ground truth | Report credibility | High | Monte Carlo uncertainty bounds; explicit confidence intervals per parcel; flag low-confidence parcels for manual review | **[HIGH]** — uncertainty quantification is standard practice. This is the single largest technical risk. |
| Perplexity API rate limits during bulk scraping | Verification pipeline stalls | Low | Batch with exponential backoff; cache all responses | **[HIGH]** — standard rate limit handling. |
| Condition scoring from remote sensing is inaccurate | BU calculation systematically biased | High | Use NDVI temporal profiles as proxy, document as experimental, provide sensitivity analysis showing BU range under different condition assumptions | **[MEDIUM]** — sensitivity analysis quantifies but does not eliminate the bias. |
| Planning portal ecology reports are scanned PDFs | Cannot extract structured data | Medium | OCR fallback via Tesseract; accept reduced extraction accuracy; flag in report | **[LOW]** — OCR on ecology report tables has poor accuracy. May need to treat these as unprocessable. |

### 1.8 Confidence Summary Matrix

| Component | Confidence | Primary Risk Factor |
|---|---|---|
| **Sentinel-2 2016 data for DE4 4AH** | MEDIUM | UK cloud cover; single-satellite revisit cadence in 2016; actual scene availability unknown until GEE query |
| **Landsat 8 2016 fallback** | HIGH | 30m resolution limits small-parcel discrimination but data exists |
| **Sentinel-2 2026 data** | HIGH | Dual-satellite constellation, mature archive, cloud-free composites routine |
| **EA LIDAR for BBOX** | MEDIUM | Coverage exists but whether 2016 flight covers this specific BBOX is unknown |
| **OS Data Hub APIs** | HIGH | Well-documented, stable, BNG-native, OAuth2 authenticated |
| **HM Land Registry INSPIRE** | HIGH | Open WFS, no auth, comprehensive coverage |
| **MAGIC Map (current)** | HIGH | Stable WMS, >15 years operational |
| **MAGIC Map (2016 historic)** | MEDIUM | WMS TIME parameter support inconsistent; no versioned historic snapshots |
| **Natural England PHI 2016** | MEDIUM | PHI is periodically updated; historic snapshots not versioned in public API |
| **Natural England PHI current** | HIGH | v2.3 is the latest published version |
| **NBN Atlas species records** | MEDIUM | API reliable but data density for specific 1km BBOX in rural Derbyshire uncertain |
| **UKHab classification accuracy** | MEDIUM | 75–85% typical for Level 3 in mixed rural; main pipeline uncertainty source |
| **Condition scoring from satellite** | LOW | Condition assessment normally requires field survey; NDVI temporal proxies are experimental |
| **BM4.0 formula implementation** | HIGH | Directly from DEFRA published specification; deterministic calculation |
| **Strategic significance from LNRS** | HIGH | Published by Derbyshire CC; mapped zones are definitive |
| **Connectivity scoring** | MEDIUM | Spatial adjacency is computable but BM4.0 connectivity criteria include subjective professional judgement elements |
| **Planning portal scraping** | LOW | Brittle; varies by council portal technology (Idox Uniform for DDDC); ecology reports may be scanned PDFs |
| **University thesis availability** | MEDIUM | Repositories exist but search APIs vary; probability of data within exact BBOX is low |
| **Perplexity API for citation discovery** | HIGH | Reliable web search API; well-documented |
| **Perplexity API accuracy of extracted facts** | MEDIUM | Web search results may contain inaccuracies; all claims require primary source verification |
| **200-page LaTeX output compilation** | HIGH | Standard toolchain; deterministic |
| **200-page LaTeX content completeness** | MEDIUM | Depends on upstream data availability; chapters may need to document gaps rather than present analysis |
| **Reflow consensus mechanism** | SPECULATIVE | Novel architecture; no precedent for this exact pattern in ecological analysis; effectiveness of adversarial re-classification is untested |
| **4-hour pipeline execution** | MEDIUM | GEE export queues and API rate limits introduce variable delays |
| **Spot-check corpus (≥3 sites)** | MEDIUM | BNG mandatory from Feb 2024; corpus is young; detailed parcel-level data in published reports is not guaranteed |

### 1.9 Known Unknowns

This section documents things we explicitly do not know at specification time, along with how each unknown will be resolved during execution.

**1. Sentinel-2 cloud cover for 2016 over DE4 4AH**
- We do not know the actual cloud-free scene count for the Apr–Sep 2016 growing season until we query GEE with `ee.ImageCollection('COPERNICUS/S2').filterBounds(bbox).filterDate('2016-04-01','2016-09-30').filter(ee.Filter.lt('CLOUDY_PIXEL_PERCENTAGE', 20))`.
- **Resolution:** Execute the GEE query in Phase 1. If fewer than 3 usable scenes, fall back to Landsat 8 30m and document the resolution degradation. If zero usable scenes from either source, the T₀ baseline chapter must rely entirely on the Natural England PHI and OS MasterMap land use classifications (no spectral classification).
- **Impact if worst case:** Chapters 4 and 6 reduced in analytical depth; classification accuracy drops; Monte Carlo uncertainty bounds widen significantly.

**2. Whether any university thesis contains fieldwork data within the exact BBOX**
- The 8 university repositories will be searched, but the probability of finding thesis-level quadrat data precisely within a 1km buffer around DE4 4AH is low. Theses typically cover landscape-scale areas (e.g., "Peak District grasslands") and may or may not include our specific site.
- **Resolution:** Execute searches in Phase 3. Accept landscape-scale data as contextual calibration. If no data found within 5km, document the absence and rely on published PHI and planning portal assessments for ground truth.
- **Impact if worst case:** FR-09 produces a thin output; the RF classifier relies solely on Natural England PHI training data, which reduces independence of validation.

**3. Condition of individual habitat parcels**
- BM4.0 condition scoring (Poor/Fairly Poor/Moderate/Fairly Good/Good) is designed for field survey by a competent ecologist. Remote sensing proxies (NDVI temporal profiles, textural metrics, phenological signatures) are experimental and not validated for BM4.0 condition categories.
- **Resolution:** Implement NDVI-based condition proxy with explicit experimental flag. Run sensitivity analysis showing BU range under all condition assumptions (Poor through Good) for each parcel. Present results as ranges, not point estimates.
- **Impact if worst case:** BU calculations have wide confidence intervals; the report must prominently caveat that condition scores are satellite-derived estimates, not field-assessed values. This fundamentally limits the report's standing as a statutory-grade assessment.

**4. Format of Derbyshire Dales planning portal ecology reports**
- We do not know whether ecology reports attached to planning applications on the Derbyshire Dales District Council Idox Uniform portal are: (a) native PDFs with extractable text and tables, (b) scanned document PDFs requiring OCR, (c) HTML pages, or (d) a mix. The BNG metric spreadsheet attachment format is also unknown.
- **Resolution:** Attempt Perplexity discovery + direct URL fetch in Phase 3. For each discovered document, detect format and apply appropriate extraction (text PDF → tabula-py; scanned → Tesseract OCR; HTML → BeautifulSoup). Flag documents where extraction confidence is below 80%.
- **Impact if worst case:** Spot-check corpus (FR-08) may yield fewer usable comparison sites than the target of ≥3. Chapter 8 may document only 1–2 cross-examinations or rely on wider-area EA/NE published surveys.

**5. EA LIDAR coverage for 2016 flight over DE4 4AH**
- The Environment Agency LIDAR programme has extensive but not universal coverage. Whether a 2016 (or adjacent year) flight covers our specific BBOX is unknown until the DEFRA Data Services Platform is queried.
- **Resolution:** Query the EA LIDAR composite index in Phase 1. If no coverage within ±1 year, the hedgerow module (FR-06) falls back to satellite-derived canopy height estimation (lower accuracy) and the report documents the limitation.
- **Impact if worst case:** Hedgerow length measurement accuracy degrades from sub-metre (LIDAR) to ~5m (satellite-derived). Hedgerow condition assessment becomes even more uncertain.

**6. Number of registered BNG gain sites near DE4 4AH**
- The DEFRA Biodiversity Gain Site Register launched in 2024. The number of registered gain sites within 5–10km of DE4 4AH is unknown and likely small given the register's youth.
- **Resolution:** Query the register API in Phase 3. If zero results, this data source contributes nothing and the spot-check corpus relies on planning portal and thesis sources alone.
- **Impact if worst case:** One fewer cross-validation source. Minor impact on overall confidence.

---

## Part 2 — Architecture Decision Records

### ADR-BDE-001: Swarm Topology — Hierarchical Mesh with Reflow Consensus

| Field | Value |
|---|---|
| Status | Proposed |
| Context | The BDE requires parallel data ingestion from 8+ APIs, independent habitat classification, and adversarial cross-validation — a single-agent pipeline would serialise these and take >12h |
| Decision | Hierarchical mesh via `claude-flow` swarm infrastructure |
| Confidence | **[MEDIUM]** — swarm infrastructure is proven for parallel task execution; the reflow consensus mechanism is novel and untested for ecological classification |

**Topology:**

```
                    ┌─────────────────────┐
                    │   Queen Orchestrator │  ← claude-flow hierarchical-coordinator
                    │   (State Machine)    │
                    └──────────┬──────────┘
                               │
            ┌──────────────────┼──────────────────┐
            ▼                  ▼                  ▼
   ┌────────────────┐ ┌───────────────┐ ┌────────────────┐
   │ Ingestion Mesh │ │ Analysis Mesh │ │ Verification   │
   │ (3–5 workers)  │ │ (2–3 workers) │ │ Mesh (3–5)     │
   │                │ │               │ │                │
   │ • GEE worker   │ │ • Classifier  │ │ • Perplexity   │
   │ • OS/MAGIC wkr │ │ • BM4.0 calc  │ │ • Citation chk │
   │ • LandReg wkr  │ │ • Stats/MC    │ │ • Cross-exam   │
   │ • LIDAR worker │ │               │ │ • LaTeX compile│
   └────────────────┘ └───────────────┘ └────────────────┘
            │                  │                  │
            └──────────────────┼──────────────────┘
                               ▼
                    ┌─────────────────────┐
                    │  Reflow Consensus   │  ← If verification flags discrepancy,
                    │  (Back-propagation) │    state reflows to Analysis Mesh for
                    └─────────────────────┘    re-classification of flagged parcels
```

**Reflow mechanism:** When a Verification worker detects >15% disagreement between AI classification and a published ecologist report for the same parcel, it emits a `REFLOW` event. The Queen re-dispatches the flagged polygon to the Analysis Mesh with additional context (the ecologist's stated habitat type) for constrained re-classification. Maximum 3 reflow cycles per parcel before escalation to human review. **[CONFIDENCE: SPECULATIVE]** — the 15% threshold is an initial heuristic with no empirical basis in this domain. The constrained re-classification approach (providing the ecologist's answer as a prior) risks confirmation bias. The effectiveness of iterative reflow for improving habitat classification accuracy is untested.

**Consequences:**
- Pro: 3–4× throughput vs serial; self-correcting classification **[CONFIDENCE: HIGH]** for throughput gain; **[CONFIDENCE: SPECULATIVE]** for self-correction effectiveness
- Pro: Each mesh is independently testable **[CONFIDENCE: HIGH]**
- Con: Increased token consumption (~2× for reflowed parcels) **[CONFIDENCE: MEDIUM]** — actual token multiplier depends on reflow frequency which is unknown
- Con: Requires careful state management in Queen — use `claude-flow` memory namespaces per mesh **[CONFIDENCE: HIGH]** — standard claude-flow capability

### ADR-BDE-002: Spatial Processing Stack

| Field | Value |
|---|---|
| Status | Proposed |
| Decision | Python + QGIS Processing API for spatial ops; all geometry in EPSG:27700 (BNG) |
| Confidence | **[HIGH]** — proven technology stack for UK spatial analysis |

**Alternatives Considered:**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Pure Python (Shapely/Rasterio/GDAL) | Lightweight, scriptable | Manual CRS handling, no visual QA | Rejected — need visual QA |
| QGIS + Python bindings (`qgis.core`) | Full geoprocessing toolbox, visual verification, native BNG support | Heavier runtime | **Selected** — we have QGIS skills, visual QA is non-negotiable for audit |
| PostGIS | Powerful spatial SQL | Overkill for single-site, adds infra | Rejected |

**Key libraries:**
- `qgis.core` / `qgis.analysis` — geoprocessing, raster analysis **[CONFIDENCE: HIGH]**
- `earthengine-api` — GEE image collection queries **[CONFIDENCE: HIGH]**
- `rasterio` — GeoTIFF I/O **[CONFIDENCE: HIGH]**
- `geopandas` — vector DataFrame operations **[CONFIDENCE: HIGH]**
- `scikit-learn` — Random Forest classifier for UKHab classification **[CONFIDENCE: HIGH]** for library availability; **[MEDIUM]** for classification accuracy (see FR-03)
- `matplotlib` + `contextily` — cartographic figure generation **[CONFIDENCE: HIGH]**

### ADR-BDE-003: BM4.0 Calculation Implementation

| Field | Value |
|---|---|
| Status | Proposed |
| Decision | Implement the full Statutory Biodiversity Metric 4.0 formula, not the simplified BU = A × D × C × S |
| Confidence | **[HIGH]** — formula is published by DEFRA with explicit score tables and worked examples |

**The actual formula per habitat parcel:**

```
BU_area = Area(ha) × Distinctiveness × Condition × Strategic_Significance × Connectivity
```

**[CONFIDENCE: HIGH]** — this is the published BM4.0 area habitat formula. The formula structure is unambiguous.

Where each factor maps to DEFRA's published score tables:

| Factor | Source | Score Range | Confidence |
|---|---|---|---|
| **Distinctiveness** | UKHab → BM4.0 Table (DEFRA 2023, Appendix B) | V.Low(0) / Low(2) / Medium(4) / High(6) / V.High(8) | **[HIGH]** — published lookup table, deterministic mapping from UKHab code |
| **Condition** | Condition assessment criteria per habitat type (BM4.0 Technical Supplement) | Poor(1) / Fairly Poor(1.5) / Moderate(2) / Fairly Good(2.5) / Good(3) | **[HIGH]** for the score values; **[LOW]** for the ability to assess condition from remote sensing (see section 1.9) |
| **Strategic Significance** | Local Nature Recovery Strategy / National Habitat Network | Low(1) / Medium(1.1) / High(1.15) | **[HIGH]** — Derbyshire LNRS is published with mapped priority zones |
| **Connectivity** | BM4.0 Connectivity Assessment (new in 4.0) | Low(1) / Medium(1.05) / High(1.1) | **[MEDIUM]** — connectivity can be computed from spatial adjacency analysis but the BM4.0 connectivity criteria include subjective professional judgement elements (e.g., "functional ecological connectivity" vs mere spatial proximity) |

**For trading/offset calculations (not primary scope but included for completeness):**

```
Post-intervention BU = Area × Distinctiveness × Condition × Strategic_Significance 
                       × Connectivity × Temporal_Multiplier × Spatial_Risk × Difficulty
```

**[CONFIDENCE: HIGH]** — published formula. Not exercised in our baseline-vs-present analysis but included for completeness.

| Factor | Purpose |
|---|---|
| Temporal Multiplier | Discounts future habitat creation (years to target condition) |
| Spatial Risk | Penalises off-site delivery |
| Difficulty of Creation | Penalises hard-to-create habitats |

**Delta calculation across all parcels:**

```
ΔBU_total = Σᵢ (BU_i(T₁) - BU_i(T₀))

where i ∈ {all parcels within 1km BBOX}
```

**[CONFIDENCE: HIGH]** — straightforward summation. The uncertainty lies in the input values, not the aggregation formula.

**Hedgerow units:** `HU = Length(km) × Distinctiveness × Condition × Strategic_Significance`

**[CONFIDENCE: HIGH]** for formula; **[MEDIUM]** for length measurement accuracy (LIDAR-dependent); **[LOW]** for hedgerow condition from remote sensing.

**Watercourse units:** `WU = Length(km) × Distinctiveness × Condition × Encroachment × Riparian_Condition`

**[CONFIDENCE: HIGH]** for formula; **[HIGH]** for length from OS VectorMap; **[LOW]** for encroachment and riparian condition from remote sensing.

### ADR-BDE-004: Citation Verification Strategy

| Field | Value |
|---|---|
| Status | Proposed |
| Decision | Dual-layer verification: Perplexity API for discovery + direct HTTP validation for every cited URL |
| Confidence | **[HIGH]** — proven approach combining AI-assisted discovery with mechanical verification |

**Layer 1 — Discovery (Perplexity API):**
- Query: published BNG assessments, ecology reports, habitat surveys within 5 km of DE4 4AH **[CONFIDENCE: HIGH]** for query execution; **[MEDIUM]** for result quality — Perplexity may surface irrelevant or outdated results
- Query: DEFRA/Natural England policy documents, BM4.0 technical supplements **[CONFIDENCE: HIGH]** — well-known documents, easily discoverable
- Query: Local Nature Recovery Strategy for Derbyshire **[CONFIDENCE: HIGH]** — published, single authoritative source
- All Perplexity responses cached with timestamp

**Layer 2 — Validation (direct HTTP):**
- Every URL in the final BibTeX file: HEAD request, verify 200 OK, store content hash **[CONFIDENCE: HIGH]** — mechanical process
- Every data source: record API endpoint, query parameters, response date, row count **[CONFIDENCE: HIGH]**
- Every figure: store SHA-256 of source data + generation script path **[CONFIDENCE: HIGH]**

**Dead link handling:** If a cited URL returns non-200 at verification time, the swarm queries Perplexity for an alternative source. If no alternative found, the citation is flagged in the report appendix with `[UNVERIFIED]` tag. **[CONFIDENCE: HIGH]** — well-defined fallback procedure.

---

## Part 3 — Domain-Driven Design Blueprint (DDD-BDE-001)

### 3.1 Bounded Contexts

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        BIODIVERSITY DELTA ENGINE                            │
│                                                                             │
│  ┌──────────────┐    ┌──────────────────┐    ┌───────────────────────────┐  │
│  │  SPATIAL      │    │  ECOLOGICAL       │    │  VERIFICATION &           │  │
│  │  INGESTION    │───▶│  ANALYSIS         │───▶│  PUBLICATION              │  │
│  │  CONTEXT      │    │  CONTEXT          │    │  CONTEXT                  │  │
│  │              │    │                  │    │                           │  │
│  │ Aggregates:  │    │ Aggregates:      │    │ Aggregates:               │  │
│  │ • SpatialTile│    │ • HabitatParcel  │    │ • SpotCheckSite           │  │
│  │ • ImageStack │    │ • BiodiversityUnit│   │ • CitationRecord          │  │
│  │ • CadastralPl│    │ • HedgerowSegment│    │ • ReportDocument          │  │
│  │ • Designation│    │ • WatercourseSegm│    │ • ReflowEvent             │  │
│  └──────────────┘    └──────────────────┘    └───────────────────────────┘  │
│         │                     │                          │                   │
│         ▼                     ▼                          ▼                   │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    SHARED KERNEL                                     │    │
│  │  • BoundingBox value object (EPSG:27700)                            │    │
│  │  • TemporalWindow (T₀, T₁)                                         │    │
│  │  • UKHabCode enum (hierarchical, Level 1→5)                         │    │
│  │  • BM4ScoreTable (immutable DEFRA lookup)                           │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

**[CONFIDENCE: HIGH]** — bounded context decomposition follows standard DDD patterns. The three contexts map cleanly to the pipeline phases (ingest → analyse → verify/publish).

### 3.2 Aggregate Roots & Entities

#### Spatial Ingestion Context

```python
@dataclass(frozen=True)
class BoundingBox:
    """EPSG:27700 British National Grid"""
    min_easting: float
    min_northing: float
    max_easting: float
    max_northing: float
    
    @classmethod
    def from_postcode_radius(cls, postcode: str, radius_m: float) -> "BoundingBox": ...

@dataclass
class SpatialTile:                      # Aggregate Root
    tile_id: UUID
    bbox: BoundingBox
    source: DataSource                  # enum: GEE_SENTINEL2, GEE_LANDSAT8, EA_LIDAR, OS_MASTERMAP
    temporal_window: TemporalWindow     # T0 or T1
    crs: str                            # always "EPSG:27700" after ingestion
    raster_path: Path | None            # GeoTIFF on disk
    vector_path: Path | None            # GeoPackage on disk
    ingested_at: datetime
    provenance: ProvenanceRecord        # API endpoint, query params, response hash

class ImageStack:                       # Aggregate Root
    stack_id: UUID
    tiles: list[SpatialTile]
    composite_path: Path                # cloud-free composite GeoTIFF
    bands: list[str]                    # ["B2","B3","B4","B8","NDVI","NDWI"]
    temporal_window: TemporalWindow
```

**[CONFIDENCE: HIGH]** — standard geospatial domain model. Data structures are well-understood.

#### Ecological Analysis Context

```python
class HabitatParcel:                    # Aggregate Root
    parcel_id: UUID
    geometry: shapely.Polygon           # EPSG:27700
    area_hectares: Decimal
    ukhab_code: UKHabCode              # e.g. UKHabCode.G3A (lowland meadow)
    classification_confidence: float    # 0.0–1.0 from Random Forest
    
    # BM4.0 scores (T₀ and T₁ variants)
    distinctiveness: Decimal            # from DEFRA lookup
    condition: Decimal                  # from condition assessment
    strategic_significance: Decimal     # from LNRS
    connectivity: Decimal               # from connectivity assessment
    
    # Computed
    biodiversity_units_t0: Decimal
    biodiversity_units_t1: Decimal
    delta_bu: Decimal                   # t1 - t0
    
    # Designations overlapping this parcel
    designations: list[Designation]     # SSSI, SAC, SPA, ancient woodland, etc.
    
    # Provenance
    source_tiles: list[UUID]            # links to SpatialTile.tile_id
    classified_at: datetime
    classification_method: str          # "RF_UKHAB_L3_v1"

class BiodiversityUnitSummary:          # Aggregate Root (site-level)
    site_id: UUID
    bbox: BoundingBox
    total_bu_t0: Decimal
    total_bu_t1: Decimal
    total_delta: Decimal
    hedgerow_units_t0: Decimal
    hedgerow_units_t1: Decimal
    hedgerow_delta: Decimal
    watercourse_units_t0: Decimal
    watercourse_units_t1: Decimal
    watercourse_delta: Decimal
    net_change_percent: Decimal         # (t1 - t0) / t0 × 100
    parcels: list[HabitatParcel]
```

**[CONFIDENCE: HIGH]** — domain model faithfully represents the BM4.0 calculation structure. The `classification_confidence` field is the key indicator for downstream uncertainty propagation.

#### Verification & Publication Context

```python
class SpotCheckSite:                    # Aggregate Root
    site_id: UUID
    planning_reference: str             # e.g. "DDDC/2023/0456"
    location: Point                     # EPSG:27700
    distance_from_centroid_m: float
    
    # Published ecologist data
    ecologist_habitat_map: dict[str, UKHabCode]  # parcel_ref → habitat type
    ecologist_bu_total: Decimal
    source_url: str
    source_accessed: datetime
    
    # AI comparison
    ai_habitat_map: dict[str, UKHabCode]
    ai_bu_total: Decimal
    agreement_percent: float            # per-parcel type match rate
    discrepancies: list[Discrepancy]
    
class ReflowEvent:                      # Domain Event
    event_id: UUID
    parcel_id: UUID
    trigger: str                        # "SPOT_CHECK_DISAGREEMENT"
    ecologist_classification: UKHabCode
    ai_classification: UKHabCode
    reflow_cycle: int                   # 1, 2, or 3 (max)
    resolution: str | None              # "RECLASSIFIED" | "CONFIRMED" | "ESCALATED"

class CitationRecord:                   # Entity
    citation_key: str                   # BibTeX key
    url: str
    http_status: int
    content_hash: str                   # SHA-256
    verified_at: datetime
    alternative_url: str | None         # if primary dead
    status: str                         # "VERIFIED" | "UNVERIFIED" | "DEAD_REPLACED"
```

**[CONFIDENCE: HIGH]** — entity structure is sound. The `ReflowEvent` domain event is well-defined even though its operational effectiveness is speculative (see ADR-BDE-001).

### 3.3 Domain Events

```
SpatialIngestionContext:
  TileIngested(tile_id, source, temporal_window)
  CompositeGenerated(stack_id, temporal_window)
  AllSourcesIngested(bbox)                          → triggers classification

EcologicalAnalysisContext:
  ParcelClassified(parcel_id, ukhab_code, confidence)
  BiodiversityUnitsCalculated(parcel_id, bu_t0, bu_t1, delta)
  SiteSummaryComputed(site_id, total_delta)         → triggers verification

VerificationContext:
  SpotCheckCompleted(site_id, agreement_percent)
  ReflowTriggered(parcel_id, cycle)                 → back to Analysis
  ReflowResolved(parcel_id, resolution)
  AllCitationsVerified(report_id)
  ReportCompiled(report_id, pdf_path)               → DONE
```

**[CONFIDENCE: HIGH]** — event flow is a direct linearisation of the pipeline phases. Event naming follows domain ubiquitous language.

### 3.4 Anti-Corruption Layers

| Boundary | ACL | Purpose | Confidence |
|---|---|---|---|
| GEE API → SpatialTile | `GeeAdapter` | Translates GEE `ee.Image` to local GeoTIFF; handles CRS reprojection; normalises band naming | **[HIGH]** — well-documented API with Python SDK |
| OS Data Hub → SpatialTile | `OrdnanceSurveyAdapter` | OAuth2 token refresh; WFS paging; GML → GeoPackage conversion | **[HIGH]** — stable API, standard OAuth2 flow |
| MAGIC Map → Designation | `MagicMapAdapter` | WMS GetFeatureInfo parsing; maps NE designation codes to domain `Designation` enum | **[HIGH]** for current data; **[MEDIUM]** for historic queries |
| Planning Portal → SpotCheckSite | `PlanningPortalScraper` | Perplexity-discovered URLs → structured extraction of BNG metric tables from PDFs/HTML | **[LOW]** — most fragile ACL; depends on document format and extraction accuracy |
| BM4.0 Lookup Tables → BM4ScoreTable | `DefraMetricAdapter` | Parses DEFRA Excel metric tool into immutable lookup; versioned (currently BM4.0 Jan 2024) | **[HIGH]** — one-time parse of published spreadsheet |

### 3.5 Ubiquitous Language

| Term | Definition |
|---|---|
| **Parcel** | A spatially discrete polygon within the BBOX, delineated by cadastral boundaries, hedgerows, watercourses, or land cover transitions |
| **Habitat Type** | UKHab classification at Level 3 minimum (e.g., g3a = Lowland meadow) |
| **Distinctiveness** | DEFRA-assigned ecological value score for a habitat type (V.Low 0 → V.High 8) |
| **Condition** | Assessment of habitat quality against type-specific criteria (Poor 1 → Good 3) |
| **Strategic Significance** | Alignment with Local Nature Recovery Strategy priorities (Low 1 → High 1.15) |
| **Connectivity** | Degree to which a parcel connects to adjacent habitats of equal/higher value |
| **Biodiversity Unit (BU)** | The composite metric: Area × Distinctiveness × Condition × Strategic Significance × Connectivity |
| **Delta (ΔBU)** | Net change in BU between T₀ and T₁ for a parcel or site |
| **Reflow** | Back-propagation of a flagged parcel from Verification to Analysis for re-classification |
| **Spot Check** | Comparison of AI-generated habitat assessment against a published ecologist report for the same or nearby site |
| **Temporal Window** | Either T₀ (2016) or T₁ (2026) — the two points of comparison |
| **BBOX** | The 1 km radial bounding box around DE4 4AH centroid in EPSG:27700 |
| **Cloud-Free Composite** | A median-pixel composite of multiple satellite passes to eliminate cloud cover |
| **UKHab** | UK Habitat Classification system (successor to Phase 1 Habitat Survey) |
| **BM4.0** | Statutory Biodiversity Metric version 4.0 (DEFRA, January 2024) |
| **LNRS** | Local Nature Recovery Strategy — county-level strategic habitat priorities |
| **BNG** | Biodiversity Net Gain (the legal requirement: ≥10% net gain) |
| **Priority Habitat** | Habitats listed under Section 41 of the NERC Act 2006 |

---

## Part 4 — Implementation Execution Plan

### 4.1 Swarm Activation Sequence

**[CONFIDENCE: HIGH]** for phase structure and worker decomposition. **[CONFIDENCE: MEDIUM]** for timing estimates — dependent on API response times and data availability.

```
Phase 1: Spatial Ingestion          (Ingestion Mesh — parallel workers)
  ├── Worker 1: GEE Sentinel-2 + Landsat 8 composites (T₀ + T₁)
  ├── Worker 2: OS Data Hub MasterMap + Places API centroid
  ├── Worker 3: MAGIC Map + Natural England PHI
  ├── Worker 4: EA LIDAR DTM/DSM
  └── Worker 5: HM Land Registry INSPIRE polygons
  
Phase 2: Classification & Calc      (Analysis Mesh — sequential dependency on Phase 1)
  ├── Worker 1: UKHab Random Forest classification (T₀ raster → parcels)
  ├── Worker 2: UKHab Random Forest classification (T₁ raster → parcels)
  ├── Worker 3: BM4.0 scoring (lookup + compute BU per parcel)
  └── Worker 4: Hedgerow + Watercourse linear feature extraction

Phase 3: Verification               (Verification Mesh — parallel, depends on Phase 2)
  ├── Worker 1–3: Perplexity API spot-check discovery + parsing
  ├── Worker 4: Cross-examination matrix generation
  └── Worker 5: Citation verification (HTTP HEAD sweep)

Phase 4: Publication                 (Single worker, depends on Phase 3)
  └── Worker 1: LaTeX compilation, figure insertion, BibTeX, PDF output
```

### 4.2 API Keys Required

| Service | Key Type | Env Var | Confidence |
|---|---|---|---|
| Google Earth Engine | Service Account JSON | `GEE_SERVICE_ACCOUNT` | **[HIGH]** — standard GEE authentication |
| Google Maps Platform | API Key | `GOOGLE_MAPS_API_KEY` | **[HIGH]** |
| OS Data Hub | OAuth2 Client ID/Secret | `OS_API_KEY` / `OS_API_SECRET` | **[HIGH]** |
| Perplexity | API Key | `PERPLEXITY_API_KEY` | **[HIGH]** |
| Natural England / MAGIC | None (open WMS/WFS) | — | **[HIGH]** — no authentication required |
| HM Land Registry INSPIRE | None (open WFS) | — | **[HIGH]** — no authentication required |
| NBN Atlas | API Key (optional) | `NBN_API_KEY` | **[HIGH]** — optional; unauthenticated access has lower rate limits but is functional |

### 4.3 Python Dependencies

**[CONFIDENCE: HIGH]** — all packages are mature, well-maintained, and available on PyPI.

```
earthengine-api>=1.4
google-cloud-storage
qgis  # via system QGIS installation + PyQGIS bindings
geopandas>=1.0
rasterio>=1.4
shapely>=2.0
scikit-learn>=1.5
matplotlib>=3.9
contextily>=1.6
fiona>=1.10
pyproj>=3.7
requests
pydantic>=2.0
```

### 4.4 LaTeX Dependencies

**[CONFIDENCE: HIGH]** — standard TeX Live distribution packages.

```
texlive-full  # or: texlive-latex-extra, texlive-science, texlive-bibtex-extra
latexmk
biber
pgfplots
booktabs
geometry
hyperref
cleveref
minted  # for Python code listings
```

---

## References

1. DEFRA (2024). *Statutory Biodiversity Metric 4.0 — Calculation Tool and User Guide*. Department for Environment, Food & Rural Affairs. **[CONFIDENCE: HIGH]** — primary statutory reference.
2. DEFRA (2024). *Statutory Biodiversity Metric 4.0 — Technical Supplement*. Condition assessment criteria per habitat type. **[CONFIDENCE: HIGH]**
3. Natural England (2023). *Priority Habitat Inventory v2.3*. Open data via MAGIC Map. **[CONFIDENCE: HIGH]** for current version; **[MEDIUM]** for 2016 historic state.
4. UK Habitat Classification Working Group (2023). *UK Habitat Classification v2.0*. **[CONFIDENCE: HIGH]**
5. Environment Act 2021, Part 6 — Biodiversity Net Gain. legislation.gov.uk. **[CONFIDENCE: HIGH]** — primary legislation.
6. Ordnance Survey (2024). *OS Data Hub API Documentation*. osdatahub.os.uk. **[CONFIDENCE: HIGH]**
7. DEFRA (2024). *Biodiversity Gain Site Register*. GOV.UK. **[CONFIDENCE: HIGH]** for existence; **[MEDIUM]** for data density near DE4 4AH.
8. Derbyshire County Council (2024). *Local Nature Recovery Strategy*. **[CONFIDENCE: HIGH]** — published county-level document.
