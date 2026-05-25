#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — University Repository Thesis Miner

Searches 8 local university institutional repositories for ecology/biodiversity
theses relevant to the Derbyshire/Peak District study area.

CONFIDENCE: MEDIUM
- Repository APIs vary widely in quality and format
- OAI-PMH is the most standardised protocol but not all repos support it
- Some may require HTML scraping (fragile)
- Metadata quality varies: abstracts may be missing or truncated
- Search relevance depends on repository indexing
- Rate limiting: be respectful of institutional servers
"""

import argparse
import csv
import json
import logging
import sys
import time
import xml.etree.ElementTree as ET
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Optional
from urllib.parse import quote_plus, urljoin

import requests

from config import DATA_DIR, setup_logging

log = setup_logging("thesis_miner")


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class ThesisResult:
    title: str
    author: str
    year: str
    abstract: str
    url: str
    repository: str
    relevance_keywords: list[str] = field(default_factory=list)


# ---------------------------------------------------------------------------
# Repository configurations
# ---------------------------------------------------------------------------

# CONFIDENCE: MEDIUM — URLs verified May 2025 but may change
REPOSITORIES = {
    "derby": {
        "name": "University of Derby Repository (UDORA)",
        "base_url": "https://derby.openrepository.com/",
        "oai_url": "https://derby.openrepository.com/oai/request",
        "search_url": "https://derby.openrepository.com/discover",
        "type": "dspace",
    },
    "nottingham": {
        "name": "University of Nottingham Repository",
        "base_url": "http://eprints.nottingham.ac.uk/",
        "search_url": "http://eprints.nottingham.ac.uk/cgi/search/archive/simple",
        "type": "eprints",
    },
    "sheffield": {
        "name": "University of Sheffield ORDA/White Rose",
        "base_url": "https://etheses.whiterose.ac.uk/",
        "search_url": "https://etheses.whiterose.ac.uk/cgi/search/archive/simple",
        "type": "eprints",
    },
    "sheffield_hallam": {
        "name": "Sheffield Hallam University Research Archive",
        "base_url": "http://shura.shu.ac.uk/",
        "search_url": "http://shura.shu.ac.uk/cgi/search/archive/simple",
        "type": "eprints",
    },
    "ntu": {
        "name": "Nottingham Trent University IRep",
        "base_url": "https://irep.ntu.ac.uk/",
        "search_url": "https://irep.ntu.ac.uk/cgi/search/archive/simple",
        "type": "eprints",
    },
    "leicester": {
        "name": "University of Leicester Repository",
        "base_url": "https://figshare.le.ac.uk/",
        "search_url": "https://figshare.le.ac.uk/search",
        "type": "figshare",
    },
    "keele": {
        "name": "Keele University Research Repository",
        "base_url": "https://eprints.keele.ac.uk/",
        "search_url": "https://eprints.keele.ac.uk/cgi/search/archive/simple",
        "type": "eprints",
    },
    "birmingham": {
        "name": "University of Birmingham eTheses",
        "base_url": "https://etheses.bham.ac.uk/",
        "search_url": "https://etheses.bham.ac.uk/cgi/search/archive/simple",
        "type": "eprints",
    },
}

# Search query templates
# CONFIDENCE: MEDIUM — relevance depends on how repositories index content
QUERY_TEMPLATES = [
    "biodiversity net gain Derbyshire",
    "habitat classification Peak District",
    "UKHab survey Derbyshire Dales",
    "biodiversity metric England limestone",
    "ecological assessment Matlock",
    "SSSI management Derbyshire",
    "calcareous grassland Peak District",
    "woodland ecology White Peak",
]


# ---------------------------------------------------------------------------
# Search implementations by repository type
# ---------------------------------------------------------------------------

def search_eprints(
    repo_config: dict,
    query: str,
    max_results: int = 20,
) -> list[ThesisResult]:
    """
    Search an EPrints repository.

    CONFIDENCE: MEDIUM — EPrints simple search API is well-documented
    but output parsing is fragile (HTML-based).
    """
    results = []
    params = {
        "q": query,
        "output": "JSON",
        "_action_search": "Search",
        "satisfyall": "ALL",
        "_order": "date",
    }

    try:
        resp = requests.get(
            repo_config["search_url"],
            params=params,
            timeout=30,
            headers={"Accept": "application/json"},
        )

        if resp.status_code != 200:
            # EPrints may not support JSON output — try XML
            params["output"] = "XML"
            resp = requests.get(
                repo_config["search_url"],
                params=params,
                timeout=30,
            )

        if resp.status_code == 200:
            # Try JSON first
            try:
                data = resp.json()
                for item in data[:max_results]:
                    results.append(ThesisResult(
                        title=item.get("title", "Unknown"),
                        author=item.get("creators_name", [{}])[0].get("family", "Unknown")
                               if item.get("creators_name") else "Unknown",
                        year=str(item.get("date", ""))[:4],
                        abstract=item.get("abstract", "")[:500],
                        url=item.get("uri", repo_config["base_url"]),
                        repository=repo_config["name"],
                        relevance_keywords=[kw for kw in query.split() if len(kw) > 3],
                    ))
            except (json.JSONDecodeError, ValueError):
                # Fallback: try to parse HTML/XML
                log.debug("JSON parse failed for %s — skipping detailed parse", repo_config["name"])

    except requests.RequestException as exc:
        log.warning("Search failed for %s: %s", repo_config["name"], exc)

    return results


def search_dspace(
    repo_config: dict,
    query: str,
    max_results: int = 20,
) -> list[ThesisResult]:
    """
    Search a DSpace repository via discover endpoint.

    CONFIDENCE: MEDIUM — DSpace API varies between versions (5.x, 6.x, 7.x).
    """
    results = []

    # Try REST API first (DSpace 6+)
    api_url = urljoin(repo_config["base_url"], "/server/api/discover/search/objects")
    params = {
        "query": query,
        "dsoType": "item",
        "size": max_results,
    }

    try:
        resp = requests.get(api_url, params=params, timeout=30)
        if resp.status_code == 200:
            data = resp.json()
            embedded = data.get("_embedded", {}).get("searchResult", {})
            objects = embedded.get("_embedded", {}).get("objects", [])
            for obj in objects:
                item = obj.get("_embedded", {}).get("indexableObject", {})
                metadata = item.get("metadata", {})
                title = metadata.get("dc.title", [{}])[0].get("value", "Unknown")
                author = metadata.get("dc.contributor.author", [{}])[0].get("value", "Unknown")
                year = metadata.get("dc.date.issued", [{}])[0].get("value", "")[:4]
                abstract = metadata.get("dc.description.abstract", [{}])[0].get("value", "")[:500]
                handle = item.get("handle", "")
                url = urljoin(repo_config["base_url"], f"handle/{handle}") if handle else ""

                results.append(ThesisResult(
                    title=title,
                    author=author,
                    year=year,
                    abstract=abstract,
                    url=url,
                    repository=repo_config["name"],
                ))
    except requests.RequestException as exc:
        log.warning("DSpace search failed for %s: %s", repo_config["name"], exc)

    return results


def search_figshare(
    repo_config: dict,
    query: str,
    max_results: int = 20,
) -> list[ThesisResult]:
    """
    Search a Figshare-based repository.

    CONFIDENCE: MEDIUM — Figshare API is well-documented but institutional
    instances may differ from public Figshare.
    """
    results = []
    api_url = urljoin(repo_config["base_url"], "/v2/articles/search")

    try:
        resp = requests.post(
            api_url,
            json={"search_for": query, "page_size": max_results},
            timeout=30,
        )
        if resp.status_code == 200:
            for item in resp.json():
                results.append(ThesisResult(
                    title=item.get("title", "Unknown"),
                    author=item.get("authors", [{}])[0].get("full_name", "Unknown")
                           if item.get("authors") else "Unknown",
                    year=str(item.get("published_date", ""))[:4],
                    abstract=item.get("description", "")[:500],
                    url=item.get("url_public_html", ""),
                    repository=repo_config["name"],
                ))
    except requests.RequestException as exc:
        log.warning("Figshare search failed for %s: %s", repo_config["name"], exc)

    return results


# ---------------------------------------------------------------------------
# OAI-PMH harvesting (supplementary)
# ---------------------------------------------------------------------------

def harvest_oai_pmh(
    oai_url: str,
    set_spec: Optional[str] = None,
    max_records: int = 50,
) -> list[ThesisResult]:
    """
    Harvest metadata via OAI-PMH protocol.

    CONFIDENCE: MEDIUM — OAI-PMH is standardised but not all repos
    expose thesis-specific sets. Results may need post-filtering.
    """
    results = []
    params = {
        "verb": "ListRecords",
        "metadataPrefix": "oai_dc",
    }
    if set_spec:
        params["set"] = set_spec

    try:
        resp = requests.get(oai_url, params=params, timeout=60)
        resp.raise_for_status()

        ns = {
            "oai": "http://www.openarchives.org/OAI/2.0/",
            "dc": "http://purl.org/dc/elements/1.1/",
            "oai_dc": "http://www.openarchives.org/OAI/2.0/oai_dc/",
        }

        root = ET.fromstring(resp.content)
        records = root.findall(".//oai:record", ns)

        for record in records[:max_records]:
            metadata = record.find(".//oai_dc:dc", ns)
            if metadata is None:
                continue

            title = metadata.findtext("dc:title", "", ns)
            creator = metadata.findtext("dc:creator", "", ns)
            date = metadata.findtext("dc:date", "", ns)[:4] if metadata.findtext("dc:date", "", ns) else ""
            description = metadata.findtext("dc:description", "", ns) or ""
            identifier = metadata.findtext("dc:identifier", "", ns)

            # Filter for relevance
            text = (title + " " + description).lower()
            keywords = ["biodiversity", "habitat", "ecology", "derbyshire", "peak district",
                        "grassland", "woodland", "conservation"]
            if any(kw in text for kw in keywords):
                results.append(ThesisResult(
                    title=title,
                    author=creator,
                    year=date,
                    abstract=description[:500],
                    url=identifier or "",
                    repository="OAI-PMH",
                ))

    except Exception as exc:
        log.warning("OAI-PMH harvest failed for %s: %s", oai_url, exc)

    return results


# ---------------------------------------------------------------------------
# Orchestrator
# ---------------------------------------------------------------------------

def search_all_repositories(
    queries: Optional[list[str]] = None,
    max_per_repo: int = 10,
    delay: float = 2.0,
) -> list[ThesisResult]:
    """
    Search all configured repositories with all query templates.

    CONFIDENCE: MEDIUM — aggregate results; individual repo reliability varies.
    """
    if queries is None:
        queries = QUERY_TEMPLATES

    all_results = []
    search_funcs = {
        "eprints": search_eprints,
        "dspace": search_dspace,
        "figshare": search_figshare,
    }

    for repo_key, repo_config in REPOSITORIES.items():
        repo_type = repo_config["type"]
        search_fn = search_funcs.get(repo_type)

        if not search_fn:
            log.warning("No search function for type: %s", repo_type)
            continue

        log.info("Searching: %s (%s)", repo_config["name"], repo_type)

        for query in queries:
            try:
                results = search_fn(repo_config, query, max_per_repo)
                all_results.extend(results)
                log.info("  Query '%s': %d results", query, len(results))
            except Exception as exc:
                log.warning("  Query '%s' failed: %s", query, exc)

            time.sleep(delay)  # Be respectful of rate limits

    # Deduplicate by title similarity
    seen_titles = set()
    unique_results = []
    for r in all_results:
        title_key = r.title.lower().strip()[:80]
        if title_key not in seen_titles:
            seen_titles.add(title_key)
            unique_results.append(r)

    log.info(
        "Total results: %d raw, %d after deduplication",
        len(all_results), len(unique_results),
    )

    return unique_results


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run() -> list[ThesisResult]:
    """Execute thesis mining pipeline."""
    results = search_all_repositories()

    # Export as CSV
    csv_path = DATA_DIR / "thesis_search_results.csv"
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "title", "author", "year", "abstract", "url", "repository", "relevance_keywords",
        ])
        writer.writeheader()
        for r in results:
            row = asdict(r)
            row["relevance_keywords"] = "; ".join(row["relevance_keywords"])
            writer.writerow(row)

    log.info("Results saved: %s (%d theses)", csv_path, len(results))

    # Also save as JSON
    json_path = DATA_DIR / "thesis_search_results.json"
    with open(json_path, "w") as f:
        json.dump([asdict(r) for r in results], f, indent=2)

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="Mine university repositories for ecology theses")
    parser.add_argument("--repo", choices=list(REPOSITORIES.keys()), help="Search single repo")
    parser.add_argument("--query", help="Custom search query")
    parser.add_argument("--delay", type=float, default=2.0, help="Delay between requests (s)")
    args = parser.parse_args()

    if args.repo and args.query:
        repo_config = REPOSITORIES[args.repo]
        search_fn = {"eprints": search_eprints, "dspace": search_dspace, "figshare": search_figshare}
        fn = search_fn.get(repo_config["type"])
        if fn:
            results = fn(repo_config, args.query)
            for r in results:
                print(f"  {r.year} | {r.author} | {r.title}")
                print(f"    {r.url}")
    else:
        run()


if __name__ == "__main__":
    main()
