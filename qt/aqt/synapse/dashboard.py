# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Synapse SvelteKit page dialogs.

Each dialog opens a Synapse SvelteKit page (served by mediasrv, see
``is_sveltekit_page`` in ``aqt/mediasrv.py``) inside a dialog-hosted
``AnkiWebView``. This mirrors how ``aqt/stats.py`` opens the ``graphs``
SvelteKit page: a ``QDialog`` owns an ``AnkiWebView`` and calls
``web.load_sveltekit_page(...)`` to render it.

Two read-only pages live here: the Memory dashboard (``synapse``) and the
AAMC coverage checker (``coverage``, PRD B4). Both share a small base dialog
so their windowing/cleanup behaviour stays identical.
"""

from __future__ import annotations

import aqt
import aqt.main
from aqt.qt import *
from aqt.utils import disable_help_button, restoreGeom, saveGeom
from aqt.webview import AnkiWebView, AnkiWebViewKind

DASHBOARD_PAGE = "synapse"
COVERAGE_PAGE = "coverage"


class _SynapsePageDialog(QDialog):
    """A dialog hosting a single read-only Synapse SvelteKit page.

    Subclasses set ``page`` (the SvelteKit page name), ``geom_key`` (the
    saved-geometry key) and ``window_title``.
    """

    page: str
    geom_key: str
    window_title: str

    def __init__(self, mw: aqt.main.AnkiQt) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        mw.garbage_collect_on_dialog_finish(self)
        self.mw = mw
        self.name = self.geom_key
        self.setWindowTitle(self.window_title)
        self.setMinimumSize(700, 500)
        disable_help_button(self)
        restoreGeom(self, self.name, default_size=(900, 700))

        self.web = AnkiWebView(kind=AnkiWebViewKind.DEFAULT)
        self.web.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding
        )

        layout = QVBoxLayout()
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self.web)
        self.setLayout(layout)

        qconnect(self.finished, self._on_finished)

        self.web.set_bridge_command(self._on_bridge_cmd, self)
        self.web.load_sveltekit_page(self.page)
        self.show()
        self.activateWindow()

    def _on_bridge_cmd(self, cmd: str) -> bool:
        # These pages are read-only; ignore any bridge callbacks.
        return False

    def _on_finished(self, _code: int) -> None:
        saveGeom(self, self.name)
        self.web.cleanup()
        self.web = None  # type: ignore[assignment]


class SynapseDashboard(_SynapsePageDialog):
    """A dialog hosting the Synapse Memory dashboard SvelteKit page."""

    page = DASHBOARD_PAGE
    geom_key = "synapseDashboard"
    window_title = "Synapse Dashboard"


class SynapseCoverage(_SynapsePageDialog):
    """A dialog hosting the Synapse AAMC coverage checker SvelteKit page."""

    page = COVERAGE_PAGE
    geom_key = "synapseCoverage"
    window_title = "Synapse Coverage"


def show_dashboard(mw: aqt.main.AnkiQt) -> SynapseDashboard:
    """Open (and return) the Synapse Memory dashboard."""
    return SynapseDashboard(mw)


def show_coverage(mw: aqt.main.AnkiQt) -> SynapseCoverage:
    """Open (and return) the Synapse AAMC coverage checker."""
    return SynapseCoverage(mw)
