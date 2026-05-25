#!/home/devuser/workspace/leila/.venv/bin/python3
"""
Biodiversity Delta Engine — Citation Verifier

Verifies all URLs referenced in the project's BibTeX file and any
other citation sources. Performs HTTP HEAD requests, records content
hashes (SHA-256), and detects dead links.

CONFIDENCE:
- Methodology: HIGH (standard URL verification)
- URL longevity: MEDIUM (government URLs tend to be stable; academic URLs less so)
- Content hashing: HIGH (SHA-256 is deterministic and collision-resistant)
- BibTeX parsing: MEDIUM (regex-based; complex nested braces may fail)
"""

import argparse
import csv
import hashlib
import logging
import re
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

import requests

from config import DATA_DIR, setup_logging

log = setup_logging("citation_verifier")

# Reasonable timeout and user agent
TIMEOUT = 15
USER_AGENT = "Leila-BDE-Citation-Verifier/1.0 (academic research; contact: anthropic@xrsystems.uk)"
MAX_RETRIES = 2
RETRY_DELAY = 3.0


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class VerificationResult:
    url: str
    status_code: int
    status_text: str
    content_hash: str
    content_length: int
    content_type: str
    redirect_url: str
    verified_at: str
    citation_key: str
    error: str = ""


# ---------------------------------------------------------------------------
# BibTeX parser
# ---------------------------------------------------------------------------

def extract_urls_from_bibtex(bib_path: Path) -> list[tuple[str, str]]:
    """
    Extract all URLs and DOIs from a BibTeX file.

    Returns list of (citation_key, url) tuples.

    CONFIDENCE: MEDIUM — regex handles standard BibTeX but may miss
    URLs in notes fields or complex nested structures.
    """
    if not bib_path.exists():
        log.warning("BibTeX file not found: %s", bib_path)
        return []

    text = bib_path.read_text(encoding="utf-8", errors="replace")
    results = []

    # Match entry keys
    entries = re.finditer(r"@\w+\{([^,]+),", text)
    entry_keys = [(m.start(), m.group(1).strip()) for m in entries]

    # Match URL fields
    url_pattern = re.compile(
        r"(?:url|howpublished|note)\s*=\s*\{([^}]*https?://[^}]*)\}",
        re.IGNORECASE,
    )

    # Match DOI fields
    doi_pattern = re.compile(
        r"doi\s*=\s*\{([^}]+)\}",
        re.IGNORECASE,
    )

    for match in url_pattern.finditer(text):
        url = match.group(1).strip()
        # Find which entry this URL belongs to
        pos = match.start()
        key = "unknown"
        for entry_pos, entry_key in reversed(entry_keys):
            if entry_pos < pos:
                key = entry_key
                break
        # Extract clean URL (may have surrounding text)
        url_clean = re.search(r"https?://[^\s,}]+", url)
        if url_clean:
            results.append((key, url_clean.group(0).rstrip(".")))

    for match in doi_pattern.finditer(text):
        doi = match.group(1).strip()
        pos = match.start()
        key = "unknown"
        for entry_pos, entry_key in reversed(entry_keys):
            if entry_pos < pos:
                key = entry_key
                break
        results.append((key, f"https://doi.org/{doi}"))

    log.info("Extracted %d URLs from %s", len(results), bib_path)
    return results


def extract_urls_from_text(text_path: Path) -> list[tuple[str, str]]:
    """
    Extract URLs from any text file (markdown, CSV, etc).

    CONFIDENCE: HIGH — straightforward regex.
    """
    if not text_path.exists():
        return []

    text = text_path.read_text(encoding="utf-8", errors="replace")
    urls = re.findall(r"https?://[^\s<>\"')\]]+", text)
    return [(f"text:{text_path.name}", url.rstrip(".,;:")) for url in urls]


# ---------------------------------------------------------------------------
# URL verification
# ---------------------------------------------------------------------------

def verify_url(
    url: str,
    citation_key: str = "",
    method: str = "HEAD",
) -> VerificationResult:
    """
    Verify a single URL with HEAD request (falling back to GET if HEAD fails).

    CONFIDENCE: HIGH for methodology.
    Some servers block HEAD requests or return misleading status codes.
    """
    headers = {"User-Agent": USER_AGENT}
    verified_at = datetime.now(timezone.utc).isoformat()

    for attempt in range(MAX_RETRIES + 1):
        try:
            if method == "HEAD":
                resp = requests.head(
                    url, headers=headers, timeout=TIMEOUT,
                    allow_redirects=True,
                )
                # Some servers return 405 for HEAD — retry with GET
                if resp.status_code == 405:
                    method = "GET"
                    continue
            else:
                resp = requests.get(
                    url, headers=headers, timeout=TIMEOUT,
                    allow_redirects=True, stream=True,
                )

            # Content hash (GET only, limited to first 10KB)
            content_hash = ""
            content_length = int(resp.headers.get("content-length", 0))

            if method == "GET":
                content = resp.content[:10240]
                content_hash = hashlib.sha256(content).hexdigest()
                content_length = len(resp.content)

            redirect_url = resp.url if resp.url != url else ""

            return VerificationResult(
                url=url,
                status_code=resp.status_code,
                status_text=resp.reason,
                content_hash=content_hash,
                content_length=content_length,
                content_type=resp.headers.get("content-type", ""),
                redirect_url=redirect_url,
                verified_at=verified_at,
                citation_key=citation_key,
            )

        except requests.Timeout:
            if attempt < MAX_RETRIES:
                time.sleep(RETRY_DELAY)
                continue
            return VerificationResult(
                url=url, status_code=0, status_text="TIMEOUT",
                content_hash="", content_length=0, content_type="",
                redirect_url="", verified_at=verified_at,
                citation_key=citation_key, error="Connection timeout",
            )
        except requests.ConnectionError as exc:
            return VerificationResult(
                url=url, status_code=0, status_text="CONNECTION_ERROR",
                content_hash="", content_length=0, content_type="",
                redirect_url="", verified_at=verified_at,
                citation_key=citation_key, error=str(exc)[:200],
            )
        except requests.RequestException as exc:
            return VerificationResult(
                url=url, status_code=0, status_text="ERROR",
                content_hash="", content_length=0, content_type="",
                redirect_url="", verified_at=verified_at,
                citation_key=citation_key, error=str(exc)[:200],
            )

    # Should not reach here
    return VerificationResult(
        url=url, status_code=0, status_text="UNKNOWN",
        content_hash="", content_length=0, content_type="",
        redirect_url="", verified_at=verified_at,
        citation_key=citation_key, error="Max retries exceeded",
    )


def verify_all_urls(
    url_pairs: list[tuple[str, str]],
    delay: float = 1.0,
) -> list[VerificationResult]:
    """
    Verify all URLs with polite delay between requests.

    CONFIDENCE: HIGH for methodology.
    """
    results = []
    total = len(url_pairs)

    for i, (key, url) in enumerate(url_pairs, 1):
        log.info("[%d/%d] Verifying: %s", i, total, url[:80])
        result = verify_url(url, citation_key=key)

        status = "OK" if 200 <= result.status_code < 400 else "DEAD"
        if result.status_code == 0:
            status = "ERROR"

        log.info(
            "  %s %d %s%s",
            status, result.status_code, result.status_text,
            f" -> {result.redirect_url[:60]}" if result.redirect_url else "",
        )
        results.append(result)

        if i < total:
            time.sleep(delay)

    # Summary
    ok = sum(1 for r in results if 200 <= r.status_code < 400)
    redirects = sum(1 for r in results if 300 <= r.status_code < 400)
    dead = sum(1 for r in results if r.status_code >= 400)
    errors = sum(1 for r in results if r.status_code == 0)

    log.info("=" * 50)
    log.info("Verification summary: %d total", total)
    log.info("  OK (2xx):       %d", ok)
    log.info("  Redirects (3xx): %d", redirects)
    log.info("  Dead (4xx/5xx):  %d", dead)
    log.info("  Errors:          %d", errors)
    log.info("=" * 50)

    return results


# ---------------------------------------------------------------------------
# Pipeline entry point
# ---------------------------------------------------------------------------

def run(
    bib_paths: Optional[list[Path]] = None,
) -> list[VerificationResult]:
    """Execute citation verification pipeline."""

    # Find .bib files
    if bib_paths is None:
        project_root = Path("/home/devuser/workspace/leila")
        bib_files = list(project_root.rglob("*.bib"))
        if not bib_files:
            log.warning("No .bib files found in project — searching for other URL sources")
    else:
        bib_files = bib_paths

    all_urls = []

    # Extract from BibTeX
    for bib in bib_files:
        urls = extract_urls_from_bibtex(bib)
        all_urls.extend(urls)

    # Also check markdown files in latex/ directory
    latex_dir = Path("/home/devuser/workspace/leila/latex")
    if latex_dir.exists():
        for md_file in latex_dir.glob("*.md"):
            urls = extract_urls_from_text(md_file)
            all_urls.extend(urls)
        for tex_file in latex_dir.glob("*.tex"):
            urls = extract_urls_from_text(tex_file)
            all_urls.extend(urls)

    if not all_urls:
        log.warning("No URLs found to verify")
        return []

    # Deduplicate by URL
    seen = set()
    unique_urls = []
    for key, url in all_urls:
        if url not in seen:
            seen.add(url)
            unique_urls.append((key, url))

    log.info("URLs to verify: %d (from %d total, %d unique)", len(unique_urls), len(all_urls), len(unique_urls))

    # Verify
    results = verify_all_urls(unique_urls)

    # Export CSV
    csv_path = DATA_DIR / "citation_verification.csv"
    with open(csv_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow([
            "citation_key", "url", "status_code", "status_text",
            "content_hash", "content_length", "content_type",
            "redirect_url", "verified_at", "error",
        ])
        for r in results:
            writer.writerow([
                r.citation_key, r.url, r.status_code, r.status_text,
                r.content_hash, r.content_length, r.content_type,
                r.redirect_url, r.verified_at, r.error,
            ])

    log.info("Verification log saved: %s", csv_path)

    # Dead links report
    dead = [r for r in results if r.status_code >= 400 or r.status_code == 0]
    if dead:
        dead_path = DATA_DIR / "dead_links.csv"
        with open(dead_path, "w", newline="", encoding="utf-8") as f:
            writer = csv.writer(f)
            writer.writerow(["citation_key", "url", "status", "error"])
            for r in dead:
                writer.writerow([r.citation_key, r.url, f"{r.status_code} {r.status_text}", r.error])
        log.warning("Dead links found: %d — see %s", len(dead), dead_path)

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="Verify citation URLs")
    parser.add_argument("--bib", type=Path, nargs="+", help="BibTeX files to check")
    parser.add_argument("--url", help="Verify a single URL")
    parser.add_argument("--delay", type=float, default=1.0, help="Delay between requests (s)")
    args = parser.parse_args()

    if args.url:
        result = verify_url(args.url)
        print(f"Status: {result.status_code} {result.status_text}")
        print(f"Hash:   {result.content_hash}")
        print(f"Type:   {result.content_type}")
        if result.redirect_url:
            print(f"Redirect: {result.redirect_url}")
        if result.error:
            print(f"Error:  {result.error}")
    else:
        run(bib_paths=args.bib)


if __name__ == "__main__":
    main()
