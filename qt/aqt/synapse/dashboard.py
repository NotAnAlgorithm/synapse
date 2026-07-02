# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""The Synapse Memory dashboard.

Opens the ``synapse`` SvelteKit page (served by mediasrv, see
``is_sveltekit_page`` in ``aqt/mediasrv.py``) inside a dialog-hosted
``AnkiWebView``. This mirrors how ``aqt/stats.py`` opens the ``graphs``
SvelteKit page: a ``QDialog`` owns an ``AnkiWebView`` and calls
``web.load_sveltekit_page(...)`` to render it.
"""

from __future__ import annotations

import aqt
import aqt.main
from aqt.qt import *
from aqt.utils import disable_help_button, restoreGeom, saveGeom
from aqt.webview import AnkiWebView, AnkiWebViewKind

DASHBOARD_PAGE = "synapse"


class SynapseDashboard(QDialog):
    """A dialog hosting the Synapse Memory dashboard SvelteKit page."""

    def __init__(self, mw: aqt.main.AnkiQt) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        mw.garbage_collect_on_dialog_finish(self)
        self.mw = mw
        self.name = "synapseDashboard"
        self.setWindowTitle("Synapse Dashboard")
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
        self.web.load_sveltekit_page(DASHBOARD_PAGE)
        self.show()
        self.activateWindow()

    def _on_bridge_cmd(self, cmd: str) -> bool:
        # The dashboard is read-only for M0; ignore any bridge callbacks.
        return False

    def _on_finished(self, _code: int) -> None:
        saveGeom(self, self.name)
        self.web.cleanup()
        self.web = None  # type: ignore[assignment]


def show_dashboard(mw: aqt.main.AnkiQt) -> SynapseDashboard:
    """Open (and return) the Synapse Memory dashboard."""
    return SynapseDashboard(mw)
