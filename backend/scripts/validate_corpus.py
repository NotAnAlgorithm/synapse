#!/usr/bin/env python3
"""Standalone corpus validator for Synapse's vetted-source corpus.

Checks that every record in a JSONL corpus file has the required fields and a
valid, openly-licensed `source.license`. This is a *structural + licensing* gate;
it deliberately does NOT judge factual correctness (that is human review) and it
makes no network calls, so it can run in CI with zero dependencies.

Usage:
    python scripts/validate_corpus.py [corpus/seed.jsonl]

Exit code 0 if every record is valid, 1 otherwise (with per-line diagnostics on
stderr). Do not run in this environment — the integrator runs it centrally.

Record contract (see corpus/README.md — keep the two in sync):
    {
      "id": str,                    # unique, non-empty slug
      "concept_tags": [str, ...],   # >=1, each "concept::<section>::<id>"
      "aamc_category": str,         # non-empty content-category code
      "text": str,                  # one atomic fact, non-empty
      "source": {
        "title": str,               # non-empty
        "section": str,             # non-empty
        "anchor": str,              # non-empty
        "license": str              # one of ALLOWED_LICENSES
      }
    }
"""

from __future__ import annotations

import json
import re
import sys

# Only owned / openly-licensed content may enter the corpus. OpenStax and other
# CC-BY sources use "CC-BY-4.0"; text authored for Synapse uses "Synapse-Original".
# Anything else (e.g. a copyright-restricted author-reference source) is rejected.
ALLOWED_LICENSES = {"CC-BY-4.0", "Synapse-Original"}

# Concept-tag convention shared with the client (qt/aqt/synapse/provision.py):
# concept::<section>::<id>, underscores only, no spaces.
CONCEPT_TAG_RE = re.compile(r"^concept::[a-z0-9_]+::[a-z0-9_]+$")

DEFAULT_PATH = "corpus/seed.jsonl"


def _validate_source(source: object, prefix: str) -> list[str]:
    """Return a list of error strings for one record's `source` object."""
    errors: list[str] = []
    if not isinstance(source, dict):
        return [f"{prefix}: 'source' must be an object"]
    for key in ("title", "section", "anchor", "license"):
        value = source.get(key)
        if not isinstance(value, str) or not value.strip():
            errors.append(f"{prefix}: source.{key} must be a non-empty string")
    license_id = source.get("license")
    if isinstance(license_id, str) and license_id not in ALLOWED_LICENSES:
        errors.append(
            f"{prefix}: source.license {license_id!r} is not allowed "
            f"(must be one of {sorted(ALLOWED_LICENSES)}) — corpus is "
            f"owned/openly-licensed only"
        )
    return errors


def _validate_record(record: object, prefix: str) -> list[str]:
    """Return a list of error strings for one parsed record."""
    errors: list[str] = []
    if not isinstance(record, dict):
        return [f"{prefix}: record must be a JSON object"]

    rec_id = record.get("id")
    if not isinstance(rec_id, str) or not rec_id.strip():
        errors.append(f"{prefix}: 'id' must be a non-empty string")

    tags = record.get("concept_tags")
    if not isinstance(tags, list) or not tags:
        errors.append(f"{prefix}: 'concept_tags' must be a non-empty array")
    else:
        for tag in tags:
            if not isinstance(tag, str) or not CONCEPT_TAG_RE.match(tag):
                errors.append(
                    f"{prefix}: concept tag {tag!r} must match "
                    f"concept::<section>::<id> (lowercase, underscores, no spaces)"
                )

    for key in ("aamc_category", "text"):
        value = record.get(key)
        if not isinstance(value, str) or not value.strip():
            errors.append(f"{prefix}: '{key}' must be a non-empty string")

    errors.extend(_validate_source(record.get("source"), prefix))
    return errors


def validate_file(path: str) -> int:
    """Validate a JSONL corpus file. Returns process exit code (0 ok, 1 fail)."""
    try:
        with open(path, encoding="utf-8") as handle:
            lines = handle.readlines()
    except OSError as exc:
        print(f"error: cannot read {path}: {exc}", file=sys.stderr)
        return 1

    all_errors: list[str] = []
    seen_ids: dict[str, int] = {}
    record_count = 0

    for lineno, raw in enumerate(lines, start=1):
        if not raw.strip():
            continue  # allow blank separator lines
        prefix = f"{path}:{lineno}"
        try:
            record = json.loads(raw)
        except json.JSONDecodeError as exc:
            all_errors.append(f"{prefix}: invalid JSON: {exc.msg}")
            continue

        record_count += 1
        record_errors = _validate_record(record, prefix)
        all_errors.extend(record_errors)

        # Duplicate-id check across the whole file (ids are chunk primary keys).
        if isinstance(record, dict):
            rec_id = record.get("id")
            if isinstance(rec_id, str) and rec_id.strip():
                if rec_id in seen_ids:
                    all_errors.append(
                        f"{prefix}: duplicate id {rec_id!r} "
                        f"(first seen on line {seen_ids[rec_id]})"
                    )
                else:
                    seen_ids[rec_id] = lineno

    if all_errors:
        for err in all_errors:
            print(err, file=sys.stderr)
        print(
            f"\nFAILED: {len(all_errors)} error(s) across {record_count} record(s).",
            file=sys.stderr,
        )
        return 1

    print(f"OK: {record_count} record(s) valid in {path}.")
    return 0


def main(argv: list[str]) -> int:
    path = argv[1] if len(argv) > 1 else DEFAULT_PATH
    return validate_file(path)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
