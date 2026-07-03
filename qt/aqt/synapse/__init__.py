# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Synapse-specific helpers layered on top of the Anki/Synapse core.

This package is intentionally lightweight. In particular, it does NOT import
``provision`` (or the Qt-touching ``dashboard``/``mint`` modules) at import
time: ``provision`` is a pure-pylib module (it only touches ``anki.*``, never
``aqt``/Qt) so that it can be exercised headless. Importing it eagerly here
would drag the aqt package graph into those tests. ``init(mw)`` imports the
submodules lazily so that ``import aqt.synapse`` stays Qt-free.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import anki.collection

if TYPE_CHECKING:
    import aqt.main


def init(mw: aqt.main.AnkiQt) -> None:
    """Wire the Synapse desktop UX into a running main window.

    Adds a Tools-menu group ("Synapse: Set up", "Synapse Dashboard",
    "Synapse Coverage" and "Synapse Graph") and installs the error-driven
    card-minting hooks. All submodules are imported lazily so that a bare
    ``import aqt.synapse`` never pulls in Qt.
    """
    from aqt.qt import QAction, qconnect

    from . import dashboard, mint

    # --- Tools menu -----------------------------------------------------------
    menu = mw.form.menuTools
    menu.addSeparator()

    setup_action = QAction("Synapse: Set up", mw)
    qconnect(setup_action.triggered, lambda: _run_provision(mw))
    menu.addAction(setup_action)

    dashboard_action = QAction("Synapse Dashboard", mw)
    qconnect(dashboard_action.triggered, lambda: dashboard.show_dashboard(mw))
    menu.addAction(dashboard_action)

    coverage_action = QAction("Synapse Coverage", mw)
    qconnect(coverage_action.triggered, lambda: dashboard.show_coverage(mw))
    menu.addAction(coverage_action)

    graph_action = QAction("Synapse Graph", mw)
    qconnect(graph_action.triggered, lambda: dashboard.show_graph(mw))
    menu.addAction(graph_action)

    exam_date_action = QAction("Synapse: Set Exam Date...", mw)
    qconnect(exam_date_action.triggered, lambda: _set_exam_date(mw))
    menu.addAction(exam_date_action)

    # --- Mint hooks -----------------------------------------------------------
    mint.install_hooks()

    # --- Auto-provision on first collection load ------------------------------
    # First run sets up the Synapse environment automatically (idempotent), so
    # users get the deck / notetype / FSRS / demo cards without hunting for the
    # "Set up" action. Gated on is_provisioned so it writes only once.
    from aqt import gui_hooks

    gui_hooks.collection_did_load.append(_auto_provision)


def _run_provision(mw: aqt.main.AnkiQt) -> None:
    """Run provisioning off the UI thread and tooltip a summary."""
    from aqt.operations import QueryOp
    from aqt.utils import tooltip

    from . import provision

    def on_success(summary: dict[str, Any]) -> None:
        added = summary.get("notes_added", 0)
        tooltip(
            f"Synapse ready - FSRS on, deck + notetype provisioned, "
            f"{added} demo note(s) added"
        )

    QueryOp(
        parent=mw,
        op=lambda col: provision.provision(col),
        success=on_success,
    ).with_progress("Setting up Synapse...").run_in_background()


def _set_exam_date(mw: aqt.main.AnkiQt) -> None:
    """Prompt for the MCAT date and toggle the test-date governor (A2).

    Stores generic collection config the scheduler reads:
    ``synapse:test_date`` (YYYY-MM-DD) + ``synapse:governor_enabled``. A blank
    date turns the governor off. The governor only ever *raises* retention in
    the final ~3 weeks before the date (never lowers it early).
    """
    import datetime

    from aqt.operations import QueryOp
    from aqt.qt import QInputDialog
    from aqt.utils import tooltip

    text, ok = QInputDialog.getText(
        mw,
        "Synapse - Exam Date",
        "Your MCAT date (YYYY-MM-DD), or leave blank to turn the governor off:",
    )
    if not ok:
        return
    text = text.strip()
    enabled = bool(text)
    if enabled:
        try:
            datetime.date.fromisoformat(text)
        except ValueError:
            tooltip("Please enter the date as YYYY-MM-DD.")
            return

    def op(col: anki.collection.Collection) -> None:
        col.set_config("synapse:governor_enabled", enabled)
        if enabled:
            col.set_config("synapse:test_date", text)

    def on_success(_: None) -> None:
        tooltip(
            f"Exam date set to {text} - deadline governor on."
            if enabled
            else "Deadline governor turned off."
        )

    QueryOp(parent=mw, op=op, success=on_success).run_in_background()


def _auto_provision(col: anki.collection.Collection) -> None:
    """Provision the Synapse environment on first collection load.

    Idempotent and defensive: skips if already provisioned, and never lets a
    failure break startup (the manual "Synapse: Set up" action remains).
    """
    from . import provision

    try:
        if provision.is_provisioned(col):
            return
        provision.provision(col)
    except Exception as exc:  # noqa: BLE001 - must not break app startup
        print(f"Synapse: auto-provision failed: {exc}")
