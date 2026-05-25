#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Published BNG Assessment Spot Checker

Cross-examines AI-generated biodiversity metric calculations against
published ecology reports from the Derbyshire Dales planning portal.

CONFIDENCE: LOW
- Planning portal scraping is brittle (HTML structure changes frequently)
- PDF table extraction has ~70% reliability (tabula/pdfplumber accuracy)
- Not all planning applications include full BNG metric tables
- AI classification comparison is approximate (different site boundaries,
  different survey dates, different methodologies)
- This module is best-effort validation, not ground truth
"""

import argparse
import csv
import json
import logging
import re
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional
from urllib.parse import urljoin

import pandas as pd
import requests

from config import DATA_DIR, setup_logging

log = setup_logging("spot_checker")

# ---------------------------------------------------------------------------
# Planning portal configuration
# ---------------------------------------------------------------------------

# Derbyshire Dales District Council planning search
# CONFIDENCE: LOW — URL structure may change without notice
DDDC_SEARCH_URL = "https://www.derbyshiredales.gov.uk/planning-a-building-control/search-planning-applications"

# Alternative: national planning portal
# CONFIDENCE: MEDIUM — Planning Portal API is more stable
PLANNING_PORTAL_SEARCH = "https://www.planningportal.co.uk/planning/planning-applications"

# Keywords for finding ecology reports
ECOLOGY_KEYWORDS = [
    "biodiversity net gain",
    "ecological appraisal",
    "biodiversity metric",
    "habitat survey",
    "preliminary ecological appraisal",
    "PEA",
    "BNG assessment",
]


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class PlanningApplication:
    reference: str
    address: str
    description: str
    decision: str
    decision_date: str
    url: str
    ecology_docs: list[dict] = field(default_factory=list)


@dataclass
class ExtractedMetric:
    application_ref: str
    habitat_type: str
    area_ha: float
    distinctiveness: float
    condition: str
    bu_value: float
    source_doc: str
    page_number: int = 0
    confidence: str = "LOW"  # Extraction confidence


# ---------------------------------------------------------------------------
# Planning portal search
# ---------------------------------------------------------------------------

def search_planning_applications(
    postcode_area: str = "DE4",
    max_results: int = 50,
) -> list[PlanningApplication]:
    """
    Search for planning applications in the DE4 area.

    CONFIDENCE: LOW — scraping is fragile; API access would be preferable
    but DDDC does not offer a public API.

    Strategy: Search for applications mentioning ecology/BNG keywords.
    """
    applications = []

    # Try Derbyshire Dales planning search
    # CONFIDENCE: LOW — HTML structure is not guaranteed
    log.info("Searching DDDC planning portal for ecology reports...")

    for keyword in ["biodiversity net gain", "ecological appraisal"]:
        params = {
            "search": f"{keyword} {postcode_area}",
            "type": "planning",
        }

        try:
            resp = requests.get(
                DDDC_SEARCH_URL,
                params=params,
                timeout=30,
                headers={"User-Agent": "Leila-BDE/1.0 (academic research)"},
            )

            if resp.status_code == 200:
                # CONFIDENCE: LOW — HTML parsing is brittle
                # Extract application references from response
                refs = re.findall(
                    r"(\d{2}/\d{5}/(?:FUL|OUT|REM|VAR|DIS|LBA))",
                    resp.text,
                )
                for ref in refs[:max_results]:
                    applications.append(PlanningApplication(
                        reference=ref,
                        address="",
                        description=f"Found via '{keyword}' search",
                        decision="",
                        decision_date="",
                        url=f"https://www.derbyshiredales.gov.uk/planning/{ref}",
                    ))
            else:
                log.warning("Planning search returned %d", resp.status_code)

        except requests.RequestException as exc:
            log.warning("Planning portal search failed: %s", exc)

        time.sleep(2)  # Be respectful

    # Deduplicate
    seen = set()
    unique = []
    for app in applications:
        if app.reference not in seen:
            seen.add(app.reference)
            unique.append(app)

    log.info("Found %d unique planning applications", len(unique))
    return unique


# ---------------------------------------------------------------------------
# PDF table extraction
# ---------------------------------------------------------------------------

def extract_metric_table_from_pdf(
    pdf_path: Path,
) -> list[ExtractedMetric]:
    """
    Extract BNG metric tables from an ecology report PDF.

    CONFIDENCE: LOW
    - PDF table extraction is unreliable (~70% accuracy)
    - Table formats vary between ecology consultancies
    - Some PDFs are image-based (scanned) and need OCR
    - This function tries multiple strategies and returns best effort
    """
    metrics = []

    # Try pdfplumber first (if available)
    try:
        import pdfplumber

        with pdfplumber.open(pdf_path) as pdf:
            for page_num, page in enumerate(pdf.pages, 1):
                tables = page.extract_tables()
                for table in tables:
                    if not table or len(table) < 2:
                        continue

                    # Look for BNG metric table signatures
                    header = [str(cell).lower() if cell else "" for cell in table[0]]
                    is_metric_table = any(
                        kw in " ".join(header)
                        for kw in ["habitat", "distinctiveness", "condition", "biodiversity"]
                    )

                    if not is_metric_table:
                        continue

                    log.info("Found metric table on page %d of %s", page_num, pdf_path.name)

                    # Parse rows
                    for row in table[1:]:
                        if not row or len(row) < 4:
                            continue
                        try:
                            metrics.append(ExtractedMetric(
                                application_ref=pdf_path.stem,
                                habitat_type=str(row[0] or "").strip(),
                                area_ha=_parse_float(row[1]),
                                distinctiveness=_parse_float(row[2]),
                                condition=str(row[3] or "").strip(),
                                bu_value=_parse_float(row[-1]) if len(row) > 4 else 0.0,
                                source_doc=pdf_path.name,
                                page_number=page_num,
                                confidence="MEDIUM",  # Table was found and parsed
                            ))
                        except (ValueError, IndexError):
                            continue

    except ImportError:
        log.warning("pdfplumber not installed — trying basic text extraction")

    # Fallback: regex on raw text
    if not metrics:
        try:
            import pdfplumber
            with pdfplumber.open(pdf_path) as pdf:
                full_text = "\n".join(page.extract_text() or "" for page in pdf.pages)
        except Exception:
            log.warning("Could not extract text from %s", pdf_path)
            return metrics

        # Look for BU values in text
        # CONFIDENCE: LOW — regex on free text is unreliable
        bu_pattern = re.compile(
            r"(\d+\.?\d*)\s*(?:ha|hectare).*?(\d+\.?\d*)\s*(?:BU|biodiversity unit)",
            re.IGNORECASE | re.DOTALL,
        )
        for match in bu_pattern.finditer(full_text):
            metrics.append(ExtractedMetric(
                application_ref=pdf_path.stem,
                habitat_type="Unknown (text extraction)",
                area_ha=float(match.group(1)),
                distinctiveness=0,
                condition="Unknown",
                bu_value=float(match.group(2)),
                source_doc=pdf_path.name,
                confidence="LOW",
            ))

    log.info("Extracted %d metric entries from %s", len(metrics), pdf_path.name)
    return metrics


def _parse_float(value) -> float:
    """Parse a float from potentially messy table cell content."""
    if value is None:
        return 0.0
    text = str(value).strip().replace(",", "")
    match = re.search(r"[-+]?\d*\.?\d+", text)
    return float(match.group()) if match else 0.0


# ---------------------------------------------------------------------------
# Comparison with AI classifications
# ---------------------------------------------------------------------------

def compare_with_ai(
    extracted_metrics: list[ExtractedMetric],
    ai_bu_path: Optional[Path] = None,
) -> pd.DataFrame:
    """
    Compare extracted ecology report metrics with AI classification results.

    CONFIDENCE: LOW
    - Comparison is approximate (different site boundaries, dates, methods)
    - Habitat type matching is fuzzy (different naming conventions)
    - Useful for order-of-magnitude validation only
    """
    if not extracted_metrics:
        log.warning("No extracted metrics to compare")
        return pd.DataFrame()

    ai_path = ai_bu_path or DATA_DIR / ".." / "output" / "tables" / "t1_bu_breakdown.csv"
    # Find the file
    for candidate in [
        Path("/home/devuser/workspace/leila/output/tables/t1_bu_breakdown.csv"),
        DATA_DIR / "t1_bu_breakdown.csv",
    ]:
        if candidate.exists():
            ai_path = candidate
            break

    if not ai_path.exists():
        log.warning("AI BU data not found: %s", ai_path)
        return pd.DataFrame()

    ai_df = pd.read_csv(ai_path)

    # Build comparison table
    rows = []
    for metric in extracted_metrics:
        # Fuzzy match habitat type
        best_match = None
        best_score = 0

        if "ukhab_label" in ai_df.columns:
            for _, ai_row in ai_df.iterrows():
                score = _fuzzy_habitat_match(
                    metric.habitat_type, ai_row["ukhab_label"]
                )
                if score > best_score:
                    best_score = score
                    best_match = ai_row

        rows.append({
            "app_ref": metric.application_ref,
            "ecologist_habitat": metric.habitat_type,
            "ecologist_area_ha": metric.area_ha,
            "ecologist_bu": metric.bu_value,
            "ecologist_condition": metric.condition,
            "ai_habitat": best_match["ukhab_label"] if best_match is not None else "No match",
            "ai_bu": best_match["biodiversity_units"] if best_match is not None else 0,
            "match_confidence": best_score,
            "extraction_confidence": metric.confidence,
        })

    comparison = pd.DataFrame(rows)
    log.info("Comparison table: %d rows", len(comparison))

    return comparison


def _fuzzy_habitat_match(ecologist_name: str, ai_name: str) -> float:
    """
    Fuzzy match between ecologist and AI habitat names.

    CONFIDENCE: LOW — simple word overlap metric.
    """
    words_a = set(ecologist_name.lower().split())
    words_b = set(ai_name.lower().split())
    if not words_a or not words_b:
        return 0.0
    overlap = len(words_a & words_b)
    return overlap / max(len(words_a), len(words_b))


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> dict:
    """Execute spot-checking pipeline."""
    results = {}

    # Search planning portal
    applications = search_planning_applications()
    results["applications"] = applications

    # Save application list
    apps_path = DATA_DIR / "planning_applications.json"
    with open(apps_path, "w") as f:
        json.dump(
            [{"ref": a.reference, "url": a.url, "description": a.description}
             for a in applications],
            f, indent=2,
        )
    log.info("Applications saved: %s", apps_path)

    # Check for any locally downloaded PDFs
    pdf_dir = DATA_DIR / "ecology_reports"
    all_metrics = []
    if pdf_dir.exists():
        for pdf in pdf_dir.glob("*.pdf"):
            metrics = extract_metric_table_from_pdf(pdf)
            all_metrics.extend(metrics)

    if all_metrics:
        # Export extracted metrics
        metrics_path = DATA_DIR / "extracted_bng_metrics.csv"
        with open(metrics_path, "w", newline="") as f:
            writer = csv.writer(f)
            writer.writerow([
                "application_ref", "habitat_type", "area_ha",
                "distinctiveness", "condition", "bu_value",
                "source_doc", "page_number", "confidence",
            ])
            for m in all_metrics:
                writer.writerow([
                    m.application_ref, m.habitat_type, m.area_ha,
                    m.distinctiveness, m.condition, m.bu_value,
                    m.source_doc, m.page_number, m.confidence,
                ])
        log.info("Extracted metrics: %s (%d entries)", metrics_path, len(all_metrics))

        # Compare with AI
        comparison = compare_with_ai(all_metrics)
        if len(comparison) > 0:
            comp_path = DATA_DIR / "spot_check_comparison.csv"
            comparison.to_csv(comp_path, index=False)
            log.info("Comparison saved: %s", comp_path)
            results["comparison"] = comparison
    else:
        log.info(
            "No ecology report PDFs found in %s — "
            "place downloaded PDFs there to enable spot-checking",
            pdf_dir,
        )

    return results


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Cross-check AI BNG assessment against published reports"
    )
    parser.add_argument("--search-only", action="store_true", help="Only search planning portal")
    parser.add_argument("--pdf", type=Path, help="Extract metrics from a single PDF")
    parser.add_argument("--compare", type=Path, help="Compare extracted metrics CSV with AI")
    args = parser.parse_args()

    if args.pdf:
        metrics = extract_metric_table_from_pdf(args.pdf)
        for m in metrics:
            print(f"  {m.habitat_type}: {m.area_ha} ha, {m.bu_value} BU ({m.confidence})")
    elif args.search_only:
        apps = search_planning_applications()
        for a in apps:
            print(f"  {a.reference}: {a.description}")
    elif args.compare:
        df = pd.read_csv(args.compare)
        metrics = [
            ExtractedMetric(**row) for _, row in df.iterrows()
        ]
        comparison = compare_with_ai(metrics)
        print(comparison.to_string())
    else:
        run()


if __name__ == "__main__":
    main()
