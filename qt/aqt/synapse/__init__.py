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

from typing import TYPE_CHECKING, Literal

import anki.collection

if TYPE_CHECKING:
    import anki.cards
    import aqt.main
    import aqt.reviewer
    from aqt.qt import QMenu


def init(mw: aqt.main.AnkiQt) -> None:
    """Wire the Synapse desktop UX into a running main window.

    Adds a Tools-menu group ("Synapse: Set up...", "Synapse Dashboard",
    "Synapse Coverage" and "Synapse Graph") and installs the error-driven
    card-minting hooks. All submodules are imported lazily so that a bare
    ``import aqt.synapse`` never pulls in Qt.
    """
    from aqt.qt import QAction, qconnect

    from . import dashboard, mint

    # --- Tools menu -----------------------------------------------------------
    menu = mw.form.menuTools
    menu.addSeparator()

    setup_action = QAction("Synapse: Set up...", mw)
    qconnect(setup_action.triggered, lambda: _show_setup(mw))
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

    generate_action = QAction("Synapse: Generate practice...", mw)
    qconnect(generate_action.triggered, lambda: _generate_practice(mw))
    menu.addAction(generate_action)

    settings_action = QAction("Synapse: Service & Sync...", mw)
    qconnect(settings_action.triggered, lambda: _show_service_settings(mw))
    menu.addAction(settings_action)

    # --- Mint hooks -----------------------------------------------------------
    mint.install_hooks()

    # --- First-run setup wizard -----------------------------------------------
    # The first time an unprovisioned profile loads we open the setup wizard so
    # the user can choose which features to enable, instead of silently
    # provisioning behind their back. Gated on is_provisioned so it triggers
    # only once; if no GUI can be shown it falls back to silent default
    # provisioning (see _first_run_setup).
    from aqt import gui_hooks

    gui_hooks.collection_did_load.append(_first_run_setup)

    # Post-answer AI offers: the tutor at a miss, generation at mastery (see
    # _on_answer_ai_offers). Self-gate on an MCAT note + a configured service, so
    # until the service is set up the reviewer behaves exactly as before.
    gui_hooks.reviewer_did_answer_card.append(_on_answer_ai_offers)

    # Persistent access: mint / generate / tutor in the reviewer "More" menu, so
    # every feature is available on the current card at any time.
    gui_hooks.reviewer_will_show_context_menu.append(_reviewer_context_menu)


def _show_setup(mw: aqt.main.AnkiQt) -> None:
    """Open the Synapse setup wizard (re-openable from the Tools menu)."""
    from . import setup

    setup.show_setup(mw)


def _generate_practice(mw: aqt.main.AnkiQt) -> None:
    """Manual grounded-generation flow (Tools menu)."""
    from . import generate

    generate.pick_concept_and_generate(mw)


def _show_service_settings(mw: aqt.main.AnkiQt) -> None:
    """Open the AI service + cloud-sync settings dialog."""
    from . import settings

    settings.show_service_settings(mw)


def _on_answer_ai_offers(
    _reviewer: aqt.reviewer.Reviewer,
    card: anki.cards.Card,
    ease: Literal[1, 2, 3, 4],
) -> None:
    """Post-answer AI offers.

    The tutor is offered at a MISS (understand the failure); generation is offered
    at MASTERY, not failure (PRD Principle 2 "difficulty must be earned" + B3
    add-then-fade). Both self-gate (MCAT-family note + a configured AI service),
    so this is a no-op until the service is set up and never blocks the reviewer.
    All three features are also always available from the reviewer "More" menu.
    """
    from aqt import mw

    if mw is None:
        return
    from . import generate, tutor

    try:
        tutor.offer_tutor_at_miss(mw, card, ease)
        generate.offer_generate_at_mastery(mw, card, ease)
    except Exception as exc:  # noqa: BLE001 - never break the reviewer
        print(f"Synapse: AI answer-offer failed: {exc}")


def _reviewer_context_menu(reviewer: aqt.reviewer.Reviewer, menu: QMenu) -> None:
    """Add persistent Synapse actions to the reviewer "More" menu.

    Makes mint / generate / tutor available on the current card at any time (not
    only via the transient at-answer offers). Only shown for MCAT-family notes;
    generate/tutor are enabled once the AI service is configured (mint works
    offline).
    """
    from aqt import mw
    from aqt.qt import qconnect

    if mw is None or reviewer.card is None:
        return
    card = reviewer.card
    note = card.note()
    notetype = note.note_type()
    if notetype is None or not notetype["name"].startswith("MCAT "):
        return

    from . import generate, mint, service_client, tutor

    concept_tag = next((t for t in note.tags if t.startswith("concept::")), None)
    configured = service_client.is_configured(mw.col)

    menu.addSeparator()

    mint_action = menu.addAction("Synapse: Mint recall card")
    qconnect(mint_action.triggered, lambda: mint.mint_from_note(mw, note.id))

    generate_action = menu.addAction("Synapse: Generate practice for this concept")
    qconnect(
        generate_action.triggered,
        lambda: generate.generate_for_concept(mw, concept_tag or "", source_card=card),
    )
    generate_action.setEnabled(bool(concept_tag) and configured)

    tutor_action = menu.addAction("Synapse: Ask the tutor")
    qconnect(tutor_action.triggered, lambda: tutor.open_tutor_for_card(mw, card))
    tutor_action.setEnabled(configured)


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


def _first_run_setup(col: anki.collection.Collection) -> None:
    """Open the setup wizard on first collection load, once per profile.

    Idempotent and defensive: skips if already provisioned, and never lets a
    failure break startup (the manual "Synapse: Set up..." action remains).

    Preferred path is the interactive wizard so the user chooses their features.
    If a GUI cannot be shown (no visible main window — e.g. a headless / import
    context), we fall back to silent default provisioning so the environment is
    still set up. The is_provisioned gate keeps this to a single run.
    """
    from . import provision

    try:
        if provision.is_provisioned(col):
            return
    except Exception as exc:  # noqa: BLE001 - must not break app startup
        print(f"Synapse: first-run check failed: {exc}")
        return

    from aqt import mw

    # No usable main window -> can't show a dialog; provision silently instead.
    if mw is None or not mw.isVisible():
        _silent_default_provision(col)
        return

    # Bind to a non-optional local so the closure below keeps a narrowed type.
    main = mw

    # Defer opening the wizard until the collection load settles, then show it.
    # If anything goes wrong showing the GUI, fall back to silent provisioning.
    def open_wizard() -> None:
        try:
            _show_setup(main)
        except Exception as exc:  # noqa: BLE001 - must not break app startup
            print(f"Synapse: could not show setup wizard: {exc}")
            if main.col is not None:
                _silent_default_provision(main.col)

    main.progress.single_shot(0, open_wizard)


def _silent_default_provision(col: anki.collection.Collection) -> None:
    """Provision with default options without any UI. Never raises."""
    from . import provision

    try:
        provision.provision(col)
    except Exception as exc:  # noqa: BLE001 - must not break app startup
        print(f"Synapse: auto-provision failed: {exc}")
