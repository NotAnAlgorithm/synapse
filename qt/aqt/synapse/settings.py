# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Synapse service & sync settings.

One place to point the client at (a) the hosted AI service (the Supabase Edge
Functions that back grounded generation + the tutor) and (b) the self-hosted
Synapse *sync server* used for cloud progress sync + login.

* AI service — writes the ``synapse:service_url`` / ``synapse:service_key`` /
  ``synapse:service_token`` generic-config keys that :mod:`service_client` reads.
  Leaving the URL blank keeps every AI affordance turned off (the base study
  loop is unaffected — the service is strictly optional).
* Cloud sync — writes Anki's native *custom sync URL* (``pm.set_custom_sync_url``)
  so File ▸ Sync logs in to your self-hosted Synapse sync server instead of
  AnkiWeb. See ``notes/SYNC_SETUP.md`` for how to run that server. Identity is
  the sync account (owner decision); the AI service trusts it later — for now it
  accepts the interim service token.
"""

from __future__ import annotations

import aqt
import aqt.main
from aqt.qt import *
from aqt.utils import disable_help_button, restoreGeom, saveGeom, tooltip

from .service_client import SERVICE_KEY_KEY, SERVICE_TOKEN_KEY, SERVICE_URL_KEY

GEOM_KEY = "synapseServiceSettings"


class SynapseServiceDialog(QDialog):
    """Configure the AI service endpoint + the custom sync (login) server."""

    def __init__(self, mw: aqt.main.AnkiQt) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        self.mw = mw
        self.mw.garbage_collect_on_dialog_finish(self)
        self.setWindowTitle("Synapse: Service & Sync")
        self.setMinimumWidth(520)
        disable_help_button(self)
        self._build_ui()
        restoreGeom(self, GEOM_KEY)

    def _build_ui(self) -> None:
        col = self.mw.col
        layout = QVBoxLayout()

        # --- AI service ------------------------------------------------------
        ai_box = QGroupBox("AI service (grounded generation + tutor)")
        ai_form = QFormLayout()
        self._url = QLineEdit(col.get_config(SERVICE_URL_KEY, default="") or "")
        self._url.setPlaceholderText("https://<project-ref>.supabase.co/functions/v1")
        self._key = QLineEdit(col.get_config(SERVICE_KEY_KEY, default="") or "")
        self._key.setEchoMode(QLineEdit.EchoMode.Password)
        self._key.setPlaceholderText("Supabase publishable/anon key")
        self._token = QLineEdit(col.get_config(SERVICE_TOKEN_KEY, default="") or "")
        self._token.setEchoMode(QLineEdit.EchoMode.Password)
        self._token.setPlaceholderText("service bearer token (optional; dev token)")
        ai_form.addRow("Service URL:", self._url)
        ai_form.addRow("Anon key:", self._key)
        ai_form.addRow("Service token:", self._token)
        ai_hint = QLabel(
            "Leave the URL blank to keep AI features off. The base study loop "
            "never depends on the service."
        )
        ai_hint.setWordWrap(True)
        ai_hint.setEnabled(False)
        ai_form.addRow(ai_hint)
        ai_box.setLayout(ai_form)
        layout.addWidget(ai_box)

        # --- Cloud sync / login ---------------------------------------------
        sync_box = QGroupBox("Cloud sync (login)")
        sync_form = QFormLayout()
        self._sync_url = QLineEdit(self.mw.pm.custom_sync_url() or "")
        self._sync_url.setPlaceholderText("https://sync.your-synapse-host.example")
        sync_form.addRow("Sync server URL:", self._sync_url)
        sync_hint = QLabel(
            "Point Synapse at your self-hosted sync server, then use File ▸ Sync "
            "to log in and sync your progress to the cloud. Leave blank for the "
            "default. See notes/SYNC_SETUP.md."
        )
        sync_hint.setWordWrap(True)
        sync_hint.setEnabled(False)
        sync_form.addRow(sync_hint)
        sync_box.setLayout(sync_form)
        layout.addWidget(sync_box)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )  # type: ignore
        qconnect(buttons.accepted, self._on_accept)
        qconnect(buttons.rejected, self.reject)
        layout.addWidget(buttons)
        self.setLayout(layout)

    def _on_accept(self) -> None:
        col = self.mw.col
        col.set_config(SERVICE_URL_KEY, self._url.text().strip())
        col.set_config(SERVICE_KEY_KEY, self._key.text().strip())
        col.set_config(SERVICE_TOKEN_KEY, self._token.text().strip())
        # Anki's native custom sync URL (empty string clears it -> AnkiWeb default).
        self.mw.pm.set_custom_sync_url(self._sync_url.text().strip())
        tooltip("Synapse service & sync settings saved", parent=self.mw)
        self.accept()

    def accept(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.accept(self)

    def reject(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.reject(self)


def show_service_settings(mw: aqt.main.AnkiQt) -> SynapseServiceDialog:
    """Open (and return) the Synapse service & sync settings dialog."""
    dialog = SynapseServiceDialog(mw)
    dialog.show()
    dialog.activateWindow()
    return dialog
