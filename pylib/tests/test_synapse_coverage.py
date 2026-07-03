# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Headless test for the Synapse AAMC coverage checker (PRD B4).

Provisions the Synapse demo environment into a temp collection (via the
pure-pylib ``aqt/aqt/synapse/provision.py`` module, loaded by file path so we
never import Qt), then calls the ``concept_coverage`` backend RPC and asserts:

* concepts the demo seeds cards for are reported as covered;
* concepts in the outline that have no card are reported as gaps;
* section/collection rollups add up.

Run with the pylib test harness (e.g. ``just test-py`` / pytest against the
built pylib). Requires codegen for the ConceptCoverage RPC to have been run,
i.e. ``col._backend.concept_coverage`` must exist.
"""

from __future__ import annotations

import importlib.util
import os
from types import ModuleType

from tests.shared import getEmptyCol


def _load_provision() -> ModuleType:
    """Load ``aqt/aqt/synapse/provision.py`` directly, bypassing the aqt
    package ``__init__`` (which imports Qt). The module is pure pylib, so this
    keeps the test headless."""
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    path = os.path.join(repo_root, "qt", "aqt", "synapse", "provision.py")
    spec = importlib.util.spec_from_file_location("synapse_provision", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_coverage_reports_covered_and_gaps() -> None:
    provision = _load_provision()
    col = getEmptyCol()
    try:
        provision.provision(col)

        resp = col._backend.concept_coverage(search="deck:Synapse")

        # The outline has expected concepts, and the demo seeds some of them.
        assert resp.expected_count > 0
        assert 0 < resp.covered_count < resp.expected_count

        # Collect (tag -> covered) across all categories.
        covered: dict[str, bool] = {}
        card_counts: dict[str, int] = {}
        for cat in resp.categories:
            for concept in cat.concepts:
                covered[concept.concept] = concept.covered
                card_counts[concept.concept] = concept.card_count

        # Concepts the demo seeds cards for are covered.
        for tag in (
            "concept::biochem::amino_acid_charge",
            "concept::biochem::enzyme_kinetics",
            "concept::physics::circuits_ohms_law",
            "concept::psych::operant_conditioning",
        ):
            assert covered.get(tag) is True, f"expected {tag} covered"
            assert card_counts.get(tag, 0) > 0

        # Outline concepts with no seeded card are gaps.
        for tag in (
            "concept::biochem::glycolysis",
            "concept::physics::kinematics",
            "concept::psych::attribution",
        ):
            assert covered.get(tag) is False, f"expected {tag} to be a gap"
            assert card_counts.get(tag, 0) == 0

        # Section rollups sum to the collection totals.
        section_covered = sum(s.covered_count for s in resp.sections)
        section_expected = sum(s.expected_count for s in resp.sections)
        assert section_covered == resp.covered_count
        assert section_expected == resp.expected_count

        # Per-category counts are internally consistent.
        for cat in resp.categories:
            actual_covered = sum(1 for c in cat.concepts if c.covered)
            assert cat.covered_count == actual_covered
            assert cat.expected_count == len(cat.concepts)
    finally:
        col.close()


def test_coverage_empty_collection_is_all_gaps() -> None:
    col = getEmptyCol()
    try:
        # No provisioning: no cards at all, so every outline concept is a gap.
        resp = col._backend.concept_coverage(search="")

        assert resp.expected_count > 0
        assert resp.covered_count == 0
        assert resp.coverage == 0.0
        for cat in resp.categories:
            assert cat.covered_count == 0
            assert all(not c.covered for c in cat.concepts)
    finally:
        col.close()
