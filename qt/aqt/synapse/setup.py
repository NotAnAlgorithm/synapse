# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Synapse setup wizard.

A native Qt dialog (``QDialog`` + checkboxes — deliberately *not* a webview)
that lets the user pick which Synapse features to enable, then drives the
options-driven provisioning in :mod:`aqt.synapse.provision`.

It is shown two ways (wired in ``aqt.synapse.__init__``):

* as a **first-run wizard** the first time an unprovisioned profile loads, and
* on demand from Synapse > "Set Up..." (re-openable any time).

The checkboxes come pre-checked with the recommended defaults, which mirror
:class:`~aqt.synapse.provision.SynapseOptions`' field defaults, so clicking OK
without changing anything reproduces the historical auto-provision behaviour.
On OK the selected :class:`SynapseOptions` are applied off the UI thread via a
``QueryOp`` and a summary tooltip is shown.
"""

from __future__ import annotations

from typing import Any

import aqt
import aqt.main
from aqt.operations import QueryOp
from aqt.qt import *
from aqt.utils import disable_help_button, restoreGeom, saveGeom, tooltip

from . import provision

GEOM_KEY = "synapseSetup"


# One row per standalone toggleable feature: the SynapseOptions attribute it
# drives, its checkbox label, and a one-line description shown beneath it.
_FEATURE_ROWS: list[tuple[str, str, str]] = [
    (
        "enable_fsrs",
        "FSRS scheduling",
        "Use the FSRS memory model for smarter, more accurate review intervals.",
    ),
    (
        "adoption_enabled",
        "Effort panel",
        "Show the adoption dashboard panel with study points and streaks.",
    ),
    (
        "install_seed_content",
        "Install demo content",
        "Seed a handful of concept-tagged MCAT demo notes to explore the features.",
    ),
]

# The graph-driven scheduling flags are grouped under a single "Advanced
# scheduling" toggle (all on / all off) so the wizard stays simple. The granular
# fields remain on SynapseOptions for callers (or future UI) that want per-flag
# control; the wizard just drives all four together.
_ADVANCED_SCHEDULING_ATTRS: tuple[str, ...] = (
    "interleave_by_concept",
    "mastery_gating",
    "trickle_down_credit",
    "metamorphosis",
)


class SynapseSetupDialog(QDialog):
    """Native setup wizard for choosing which Synapse features to enable."""

    def __init__(self, mw: aqt.main.AnkiQt) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        self.mw = mw
        self.mw.garbage_collect_on_dialog_finish(self)
        self.setWindowTitle("Synapse Setup")
        self.setMinimumWidth(460)
        disable_help_button(self)

        # feature attr name -> its checkbox, built from _FEATURE_ROWS.
        self._checks: dict[str, QCheckBox] = {}

        self._build_ui()
        restoreGeom(self, GEOM_KEY)

    # --- UI construction -----------------------------------------------------

    def _build_ui(self) -> None:
        defaults = provision.SynapseOptions()
        layout = QVBoxLayout()

        intro = QLabel(
            "Choose which Synapse features to turn on. The recommended options "
            "are pre-selected; you can reopen this wizard any time from "
            "Synapse > “Set Up…”."
        )
        intro.setWordWrap(True)
        layout.addWidget(intro)

        # --- Feature checkboxes ---------------------------------------------
        features_box = QGroupBox("Features")
        features_layout = QVBoxLayout()
        for attr, label, description in _FEATURE_ROWS:
            check = QCheckBox(label)
            check.setChecked(bool(getattr(defaults, attr)))
            features_layout.addWidget(check)

            hint = QLabel(description)
            hint.setWordWrap(True)
            hint.setEnabled(False)  # muted, description-style
            hint.setContentsMargins(24, 0, 0, 6)
            features_layout.addWidget(hint)

            self._checks[attr] = check

        # Single toggle for the four graph-driven scheduling flags (all together).
        self._advanced_check = QCheckBox("Advanced scheduling")
        self._advanced_check.setChecked(
            all(bool(getattr(defaults, attr)) for attr in _ADVANCED_SCHEDULING_ATTRS)
        )
        features_layout.addWidget(self._advanced_check)
        advanced_hint = QLabel(
            "Concept interleaving, mastery gating, trickle-down credit, and card "
            "metamorphosis — the prerequisite-graph-driven scheduling suite. "
            "Turned on or off together."
        )
        advanced_hint.setWordWrap(True)
        advanced_hint.setEnabled(False)
        advanced_hint.setContentsMargins(24, 0, 0, 6)
        features_layout.addWidget(advanced_hint)

        features_box.setLayout(features_layout)
        layout.addWidget(features_box)

        # --- Exam-date governor ---------------------------------------------
        governor_box = QGroupBox("Exam-date governor")
        governor_layout = QVBoxLayout()

        self._governor_check = QCheckBox("Ramp retention toward your exam date")
        self._governor_check.setChecked(defaults.governor_enabled)
        governor_layout.addWidget(self._governor_check)

        governor_hint = QLabel(
            "Gradually raises target retention in the final weeks before your "
            "MCAT. Never lowers it early."
        )
        governor_hint.setWordWrap(True)
        governor_hint.setEnabled(False)
        governor_hint.setContentsMargins(24, 0, 0, 6)
        governor_layout.addWidget(governor_hint)

        date_row = QHBoxLayout()
        date_label = QLabel("Exam date:")
        date_row.addWidget(date_label)
        self._date_edit = QDateEdit()
        self._date_edit.setCalendarPopup(True)
        self._date_edit.setDisplayFormat("yyyy-MM-dd")
        # Default to ~3 months out so the field holds a sensible date.
        self._date_edit.setDate(QDate.currentDate().addMonths(3))
        date_row.addWidget(self._date_edit)
        date_row.addStretch(1)
        governor_layout.addLayout(date_row)

        governor_box.setLayout(governor_layout)
        layout.addWidget(governor_box)

        # The date field is only meaningful when the governor is enabled.
        qconnect(self._governor_check.toggled, self._sync_governor_enabled)
        self._sync_governor_enabled(self._governor_check.isChecked())

        # --- Buttons ---------------------------------------------------------
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )  # type: ignore
        qconnect(buttons.accepted, self._on_accept)
        qconnect(buttons.rejected, self.reject)
        layout.addWidget(buttons)

        self.setLayout(layout)

    def _sync_governor_enabled(self, enabled: bool) -> None:
        self._date_edit.setEnabled(enabled)

    # --- Options + apply -----------------------------------------------------

    def _collect_options(self) -> provision.SynapseOptions:
        """Build a SynapseOptions from the current widget state."""
        governor_enabled = self._governor_check.isChecked()
        test_date = (
            self._date_edit.date().toString("yyyy-MM-dd") if governor_enabled else None
        )
        # The single "Advanced scheduling" toggle drives all four graph-driven
        # scheduling flags at once.
        advanced = self._advanced_check.isChecked()
        return provision.SynapseOptions(
            enable_fsrs=self._checks["enable_fsrs"].isChecked(),
            interleave_by_concept=advanced,
            mastery_gating=advanced,
            trickle_down_credit=advanced,
            metamorphosis=advanced,
            adoption_enabled=self._checks["adoption_enabled"].isChecked(),
            governor_enabled=governor_enabled,
            test_date=test_date,
            install_seed_content=self._checks["install_seed_content"].isChecked(),
        )

    def _on_accept(self) -> None:
        opts = self._collect_options()
        # Bind to a local so the callback doesn't capture the (about-to-close)
        # dialog; provisioning runs against the main window, not this dialog.
        mw = self.mw

        def on_success(summary: dict[str, Any]) -> None:
            added = summary.get("notes_added", 0)
            item_added = summary.get("item_notes_added", 0)
            tooltip(
                f"Synapse ready — features applied, "
                f"{added + item_added} demo note(s) added",
                parent=mw,
            )
            # Reflect new deck / notetypes / config in the main window.
            mw.reset()

        QueryOp(
            parent=mw,
            op=lambda col: provision.provision_with_options(col, opts),
            success=on_success,
        ).with_progress("Setting up Synapse...").run_in_background()

        self.accept()

    # --- Geometry ------------------------------------------------------------

    def accept(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.accept(self)

    def reject(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.reject(self)


def show_setup(mw: aqt.main.AnkiQt) -> SynapseSetupDialog:
    """Open (and return) the Synapse setup wizard."""
    dialog = SynapseSetupDialog(mw)
    dialog.show()
    dialog.activateWindow()
    return dialog
