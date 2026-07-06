# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""State-grounded conversational Socratic tutor (PRD C2).

When a student answers an MCAT-family item *Again* (ease 1), we treat the miss
as a knowledge gap and *proactively but unobtrusively* offer a Socratic tutor
grounded in (a) the item's verified ``Explanation`` and (b) the student's
per-concept mastery map. The tutor is a real, multi-turn conversation: the
student can ask follow-up questions and clarifications, and the tutor answers
using the prior conversation *and* the live card state (whether the answer side
has been flipped/revealed in the reviewer). See notes/M3_tutor_design.md §1, §3.

Design constraints (M2 §1-2, M3 §5.1 "degrade cleanly"):

* The Rust core makes **no** network calls. The *client* assembles the
  student-state bundle from local read-models (``concept_mastery`` RPC + the
  note's ``Explanation`` field) and POSTs it to the hosted service over HTTPS
  via :mod:`aqt.synapse.service_client`.
* The service is **stateless per turn**: the client carries the whole
  conversation and re-sends it on every turn (``messages`` in the payload).
* The base study loop **never depends on the tutor**. The affordance is
  dismissible and non-blocking; if the service is not configured or a call
  fails, we degrade to a quiet "tutor unavailable" state and the review
  continues (the mint path from the same miss is 100% local and unaffected).
* All HTTPS is BLOCKING, so both the bundle assembly (a local RPC read) and
  every tutor turn run off the UI thread through ``QueryOp``.

Public entry points (wired by ``aqt.synapse.__init__`` — the integrator owns
all menu/hook wiring; this module never touches ``__init__`` or the hooks):

* :func:`open_tutor_for_card` ``(mw, card) -> None`` — assemble the grounding
  context for a card, open the conversational panel, and auto-request the
  opening turn.
* :func:`offer_tutor_at_miss` ``(mw, card, ease) -> None`` — on an ``ease == 1``
  miss of an MCAT-family note (and only when the service is configured), show a
  dismissible affordance that opens the tutor for that card.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

import aqt
import aqt.main
from anki.cards import Card
from anki.collection import Collection
from anki.notes import Note, NoteId
from aqt.operations import QueryOp
from aqt.qt import *
from aqt.utils import disable_help_button, restoreGeom, saveGeom, tooltip

from . import service_client
from .provision import (
    DATA_SNIPPET_NOTETYPE_NAME,
    EXPLAIN_WHY_NOTETYPE_NAME,
    MCAT_NOTETYPE_NAME,
    SYNAPSE_DECK_NAME,
    WHICH_PRINCIPLE_NOTETYPE_NAME,
)

GEOM_KEY = "synapseTutor"

# The MCAT "family": the flagship Application notetype plus the M1 richer item
# notetypes. A miss on any of these is eligible for the tutor. (Explain-Why has
# no Explanation field -- see _explanation_of -- and simply yields no grounding,
# so the offer is skipped for it; see offer_tutor_at_miss.)
_MCAT_NOTETYPES: frozenset[str] = frozenset(
    {
        MCAT_NOTETYPE_NAME,
        WHICH_PRINCIPLE_NOTETYPE_NAME,
        DATA_SNIPPET_NOTETYPE_NAME,
        EXPLAIN_WHY_NOTETYPE_NAME,
    }
)

# Search scope for the mastery rollup -- the same card population the dashboard
# uses, so the tutor and the dashboard agree on the numbers (M3 §2.2).
_MASTERY_SEARCH = f'deck:"{SYNAPSE_DECK_NAME}"'

# Field-name candidates mapping across the MCAT notetypes (provision.py):
#   * MCAT Application:    Stem / Answer / Explanation
#   * MCAT Which-Principle: Stem / Answer / Explanation
#   * MCAT Data-Snippet:   Question / Answer / Explanation
#   * MCAT Explain-Why:    Prompt / (no Answer) / ModelAnswer
_QUESTION_FIELDS: tuple[str, ...] = ("Stem", "Question", "Prompt")
_ANSWER_FIELDS: tuple[str, ...] = ("Answer", "ModelAnswer")
_EXPLANATION_FIELDS: tuple[str, ...] = ("Explanation", "ModelAnswer")


# --- Bundle assembly (pure, runs on the collection thread) -------------------


def _is_mcat_note(note: Note) -> bool:
    notetype = note.note_type()
    return notetype is not None and notetype["name"] in _MCAT_NOTETYPES


def _concept_tags(note: Note) -> list[str]:
    """The note's ``concept::<section>::<id>`` tags (may be several; may be empty)."""
    return [t for t in note.tags if t.startswith("concept::")]


def _first_field(note: Note, candidates: tuple[str, ...]) -> str:
    """First non-empty value among ``candidates`` present on the note, else ""."""
    for field in candidates:
        if field in note:
            text = note[field].strip()
            if text:
                return text
    return ""


def _explanation_of(note: Note) -> str:
    """The item's verified explanation -- the tutor's answer-side grounding.

    Prefers the ``Explanation`` field (MCAT Application + the typed-answer item
    notetypes); falls back to ``ModelAnswer`` (Explain-Why). Empty when neither
    is present -- the tutor then has nothing to ground on and is skipped.
    """
    return _first_field(note, _EXPLANATION_FIELDS)


def _question_of(note: Note) -> str:
    """The item's question/stem, sent to the tutor as context.

    Maps across the MCAT notetypes: ``Stem`` (Application, Which-Principle),
    ``Question`` (Data-Snippet), ``Prompt`` (Explain-Why). Best-effort context
    only; empty is fine (the tutor still grounds on the explanation).
    """
    return _first_field(note, _QUESTION_FIELDS)


def _answer_of(note: Note) -> str:
    """The item's correct answer.

    Server-side, the answer is placed in the prompt ONLY when the card is
    revealed; otherwise it is used solely for the giveaway post-check. Maps
    ``Answer`` (Application, Which-Principle, Data-Snippet) and ``ModelAnswer``
    (Explain-Why). Empty when neither is present.
    """
    return _first_field(note, _ANSWER_FIELDS)


def _state_of(state: Any) -> dict[str, Any]:
    """Project one proto ``ConceptState`` into the JSON the service expects."""
    return {
        "concept": state.concept,
        "section": state.section,
        "memory": state.memory,
        "card_count": state.card_count,
        "scored_card_count": state.scored_card_count,
        "sufficient_data": state.sufficient_data,
        "mastered": state.mastered,
        "has_cards": state.has_cards,
    }


def _iter_bundles(response: Any) -> list[Any]:
    """Unwrap the ConceptMastery result into a list of bundles.

    The generated backend method returns a ``Sequence[...Bundle]`` directly, but
    be defensive: some codegen shapes hand back the wrapping response message
    with a ``.bundles`` repeated field. Handle both. (Noted for the integrator:
    if lint flags the shape, the ``Sequence`` branch is the live one.)
    """
    if response is None:
        return []
    bundles = getattr(response, "bundles", None)
    if bundles is not None:
        return list(bundles)
    return list(response)


def _bundle_for_concept(bundle: Any) -> dict[str, Any]:
    """Project one proto ``Bundle`` (focus + prerequisites) into service JSON."""
    focus = bundle.focus if bundle.HasField("focus") else None
    return {
        "focus": _state_of(focus) if focus is not None else None,
        "prerequisites": [_state_of(p) for p in bundle.prerequisites],
    }


def _assemble_context(col: Collection, note_id: NoteId) -> dict[str, Any] | None:
    """Collection-thread body: build the grounding context for the tutor thread.

    Re-fetches the note by id on the collection thread (matching ``mint.py``'s
    discipline of not carrying a UI-thread note into the background op). Returns
    the *thread-invariant* grounding fields (concept, explanation, question,
    answer, mastery bundle); the per-turn ``card_revealed`` flag and ``messages``
    are added by the caller at send time. Returns ``None`` when there is nothing
    to ground on (no concept tag, or no explanation text) so the caller can
    degrade quietly instead of firing an ungroundable request.
    """
    note = col.get_note(note_id)
    concepts = _concept_tags(note)
    explanation = _explanation_of(note)
    if not concepts or not explanation:
        return None

    response = col._backend.concept_mastery(concepts=concepts, search=_MASTERY_SEARCH)
    bundles = _iter_bundles(response)
    # The "primary" concept is the first tag; its bundle carries the weakest
    # prerequisite the tutor surfaces (M3 §1.2).
    primary = bundles[0] if bundles else None

    return {
        "concept": concepts[0],
        "item_explanation": explanation,
        "item_question": _question_of(note),
        "item_answer": _answer_of(note),
        "mastery_bundle": _bundle_for_concept(primary) if primary is not None else {},
    }


# --- Reply extraction --------------------------------------------------------


def _reply_from_response(data: dict[str, Any]) -> str:
    """Pull the assistant reply text out of a tutor turn response.

    The endpoint returns ``{reply: "..."}``; also accept the legacy
    ``{turns: [{role, content}, ...]}`` shape defensively, taking the last
    assistant turn.
    """
    reply = data.get("reply")
    if isinstance(reply, str) and reply.strip():
        return reply.strip()

    turns = data.get("turns")
    if isinstance(turns, list):
        for turn in reversed(turns):
            if isinstance(turn, dict):
                content = turn.get("content")
                if isinstance(content, str) and content.strip():
                    return content.strip()
            elif isinstance(turn, str) and turn.strip():
                return turn.strip()
    return ""


def _concept_label(tag: str) -> str:
    """Human-readable leaf of a ``concept::<section>::<id>`` tag."""
    parts = [p for p in tag.split("::") if p]
    leaf = parts[-1] if parts else tag
    return leaf.replace("_", " ").strip() or tag


# --- The conversational tutor panel ------------------------------------------


class SynapseTutorDialog(QDialog):
    """A non-modal, conversational Socratic tutor panel.

    A scrollable transcript of alternating student / tutor turns plus a
    multi-line input and Send button. Deliberately a plain ``QDialog`` (not a
    webview): every turn is a client-shell HTTPS call assembled here and run off
    the UI thread via ``QueryOp``, so no in-panel API access is needed.

    Non-modal (a ``Window``) so the student can flip the card behind the panel
    and then ask about the revealed answer -- which is why ``card_revealed`` is
    read *fresh from the live reviewer* on every send (see :meth:`_send`).
    """

    def __init__(
        self,
        mw: aqt.main.AnkiQt,
        context: dict[str, Any],
    ) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        self.mw = mw
        # Thread-invariant grounding fields (concept, explanation, question,
        # answer, mastery bundle). card_revealed + messages are added per turn.
        self._context = context
        # The conversation so far (excludes the system prompt), carried to the
        # stateless server on every turn.
        self._history: list[dict[str, str]] = []
        # Guards against overlapping requests while one turn is in flight.
        self._in_flight = False
        mw.garbage_collect_on_dialog_finish(self)
        self.setWindowTitle("Synapse Tutor")
        self.setMinimumWidth(480)
        self.setMinimumHeight(420)
        disable_help_button(self)
        self._build_ui()
        restoreGeom(self, GEOM_KEY)

    # --- UI construction -----------------------------------------------------

    def _build_ui(self) -> None:
        layout = QVBoxLayout()

        concept = str(self._context.get("concept", ""))
        heading = QLabel(f"Tutor — {_concept_label(concept)}")
        font = heading.font()
        font.setBold(True)
        heading.setFont(font)
        layout.addWidget(heading)

        # Scrollable transcript. QTextBrowser wraps long text and scrolls; we
        # append richly-formatted turns via _append_turn.
        self._transcript = QTextBrowser()
        self._transcript.setOpenExternalLinks(False)
        self._transcript.setMinimumHeight(260)
        layout.addWidget(self._transcript, 1)

        # Multi-line input + Send. Ctrl+Return sends (see eventFilter).
        self._input = QPlainTextEdit()
        self._input.setPlaceholderText(
            "Ask a follow-up question…  (Ctrl+Enter to send)"
        )
        self._input.setMinimumHeight(64)
        self._input.setMaximumHeight(140)
        self._input.installEventFilter(self)
        layout.addWidget(self._input)

        button_row = QHBoxLayout()
        button_row.addStretch(1)
        self._send_button = QPushButton("Send")
        qconnect(self._send_button.clicked, self._send)
        button_row.addWidget(self._send_button)
        layout.addLayout(button_row)

        self.setLayout(layout)

    # --- Transcript rendering ------------------------------------------------

    def _append_turn(self, role: str, text: str) -> None:
        """Append one turn to the transcript with readable, wrapped styling."""
        speaker = "You" if role == "user" else "Tutor"
        colour = "#2b6cb0" if role == "user" else "#2f855a"
        # Escape so user/tutor text can't inject markup; keep newlines readable.
        safe = _escape_html(text).replace("\n", "<br>")
        block = (
            f'<p style="margin:0 0 12px 0;">'
            f'<b style="color:{colour};">{speaker}:</b> '
            f'<span style="white-space:pre-wrap;">{safe}</span>'
            f"</p>"
        )
        self._transcript.append(block)
        # Keep the newest turn in view.
        bar = self._transcript.verticalScrollBar()
        if bar is not None:
            bar.setValue(bar.maximum())

    def _set_busy(self, busy: bool) -> None:
        """Disable/enable the input + Send while a request is in flight."""
        self._in_flight = busy
        self._input.setEnabled(not busy)
        self._send_button.setEnabled(not busy)
        self._send_button.setText("Sending…" if busy else "Send")
        if not busy:
            self._input.setFocus()

    # --- Input handling ------------------------------------------------------

    def eventFilter(self, obj: QObject | None, event: QEvent | None) -> bool:
        """Ctrl+Return (or Ctrl+Enter) in the input box sends the turn."""
        if (
            obj is self._input
            and isinstance(event, QKeyEvent)
            and event.type() == QEvent.Type.KeyPress
        ):
            key = event.key()
            mods = event.modifiers()
            is_return = key in (Qt.Key.Key_Return, Qt.Key.Key_Enter)
            if is_return and (mods & Qt.KeyboardModifier.ControlModifier):
                self._send()
                return True
        return QDialog.eventFilter(self, obj, event)

    # --- Turn dispatch -------------------------------------------------------

    def request_opening_turn(self) -> None:
        """Fire the proactive opening turn (empty conversation)."""
        self._dispatch()

    def _send(self) -> None:
        """Send the student's typed message as the next turn."""
        if self._in_flight:
            return
        text = self._input.toPlainText().strip()
        if not text:
            return
        self._input.clear()
        self._history.append({"role": "user", "content": text})
        self._append_turn("user", text)
        self._dispatch()

    def _dispatch(self) -> None:
        """Assemble the payload (with FRESH card state) and call the tutor.

        Runs the blocking HTTPS turn off the UI thread via ``QueryOp``. The
        conversation is carried in full (stateless server); ``card_revealed`` is
        read fresh from the live reviewer each time so a student who flips the
        card behind this non-modal panel gets an answer-aware reply.
        """
        self._set_busy(True)

        mw = self.mw
        # Read card state FRESH from the live reviewer on every turn.
        card_revealed = (
            getattr(getattr(mw, "reviewer", None), "state", None) == "answer"
        )
        payload: dict[str, Any] = dict(self._context)
        payload["card_revealed"] = card_revealed
        # Snapshot the history so the background op does not race a later edit.
        payload["messages"] = list(self._history)

        def op(col: Collection) -> dict[str, Any]:
            return service_client.tutor_turn(col, payload)

        QueryOp(parent=self, op=op, success=self._on_turn).failure(
            self._on_turn_failure
        ).run_in_background()

    def _on_turn(self, response: dict[str, Any]) -> None:
        self._set_busy(False)
        reply = _reply_from_response(response)
        if not reply:
            tooltip("The tutor had no guidance to offer", parent=self.mw)
            return
        self._history.append({"role": "assistant", "content": reply})
        self._append_turn("assistant", reply)

    def _on_turn_failure(self, exc: Exception) -> None:
        # Best-effort: the base loop never depends on the tutor (M3 §5.1).
        self._set_busy(False)
        if isinstance(exc, service_client.ServiceNotConfigured):
            tooltip("Synapse tutor is not configured", parent=self.mw)
        else:
            tooltip("Tutor unavailable right now", parent=self.mw)

    # --- Geometry ------------------------------------------------------------

    def reject(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.reject(self)


def _escape_html(text: str) -> str:
    """Minimal HTML escaping for transcript text."""
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


# --- The dismissible at-miss affordance --------------------------------------


class _TutorOfferBanner(QWidget):
    """A small, non-modal, dismissible banner offering the tutor at a miss.

    Proactive-but-never-blocking (M3 §4.1, §5): it floats over the reviewer, has
    an "Ask the tutor" button and a dismiss (x), and auto-hides after a while so
    a student mid-crunch can simply ignore it. It never intercepts the review
    flow or steals focus.
    """

    _AUTO_DISMISS_MS = 8000

    def __init__(
        self, parent: QWidget, concept: str, on_ask: Callable[[], None]
    ) -> None:
        QWidget.__init__(self, parent, Qt.WindowType.ToolTip)
        self._on_ask = on_ask

        row = QHBoxLayout()
        row.setContentsMargins(12, 8, 8, 8)
        label = QLabel(f"Ask the tutor about {_concept_label(concept)}?")
        row.addWidget(label)

        ask = QPushButton("Ask the tutor")
        qconnect(ask.clicked, self._ask)
        row.addWidget(ask)

        dismiss = QToolButton()
        dismiss.setText("×")  # multiplication sign, a light "close" glyph
        dismiss.setAutoRaise(True)
        qconnect(dismiss.clicked, self.close)
        row.addWidget(dismiss)

        self.setLayout(row)
        self._apply_frame_style()

        # Auto-dismiss so the affordance is transient, never a lingering blocker.
        self._timer = QTimer(self)
        self._timer.setSingleShot(True)
        self._timer.setInterval(self._AUTO_DISMISS_MS)
        qconnect(self._timer.timeout, self.close)
        self._timer.start()

    def _apply_frame_style(self) -> None:
        # A faint frame so it reads as a floating card, matching tooltip styling.
        self.setStyleSheet(
            "QWidget { border: 1px solid palette(mid); border-radius: 6px; }"
            "QLabel { border: none; }"
        )

    def _ask(self) -> None:
        self.close()
        self._on_ask()

    def show_near(self, anchor: QWidget) -> None:
        """Position bottom-right over the reviewer, then show without stealing focus."""
        self.adjustSize()
        geo = anchor.geometry()
        bottom_right = anchor.mapToGlobal(
            QPoint(geo.width() - self.width() - 24, geo.height() - self.height() - 24)
        )
        self.move(bottom_right)
        self.show()


# --- Public entry points -----------------------------------------------------


# Keep the most-recent tutor panel alive across the QueryOp callbacks (a local
# would be garbage-collected the moment open_tutor_for_card returns, closing the
# non-modal panel instantly).
_active_dialog: SynapseTutorDialog | None = None


def open_tutor_for_card(mw: aqt.main.AnkiQt, card: Card) -> None:
    """Assemble the grounding context for ``card`` and open the tutor panel.

    Runs the (local) context assembly off the UI thread via ``QueryOp``, then
    opens the non-modal conversational panel and auto-requests the opening turn.
    Degrades quietly if the service is off or errors.
    """
    if not service_client.is_configured(mw.col):
        tooltip("Synapse tutor is not configured", parent=mw)
        return

    note = card.note()
    if not _is_mcat_note(note):
        return
    note_id = note.id

    def op(col: Collection) -> dict[str, Any] | None:
        # Local RPC read only; the tutor turns themselves are fired from the
        # panel (each off the UI thread via its own QueryOp).
        return _assemble_context(col, note_id)

    def on_success(context: dict[str, Any] | None) -> None:
        global _active_dialog
        if context is None:
            tooltip("Nothing for the tutor to work with here", parent=mw)
            return
        dialog = SynapseTutorDialog(mw, context)
        _active_dialog = dialog
        dialog.show()
        dialog.activateWindow()
        # Auto-fire the proactive opening turn (empty conversation).
        dialog.request_opening_turn()

    def on_failure(exc: Exception) -> None:
        # Best-effort: the base loop never depends on the tutor (M3 §5.1).
        if isinstance(exc, service_client.ServiceNotConfigured):
            tooltip("Synapse tutor is not configured", parent=mw)
        else:
            tooltip("Tutor unavailable right now", parent=mw)

    QueryOp(parent=mw, op=op, success=on_success).failure(on_failure).with_progress(
        "Preparing the Synapse tutor..."
    ).run_in_background()


# Keep the most-recent offer banner alive (a local would be garbage-collected
# the moment offer_tutor_at_miss returns, closing the banner instantly).
_active_offer: _TutorOfferBanner | None = None


def offer_tutor_at_miss(mw: aqt.main.AnkiQt, card: Card, ease: int) -> None:
    """On an MCAT-family miss (ease 1), show a dismissible tutor offer.

    Non-blocking and best-effort: only fires when the service is configured and
    the note is an MCAT-family item with something to ground on. Never blocks
    the reviewer (M3 §4.1). Intended to be appended to
    ``gui_hooks.reviewer_did_answer_card`` -- the integrator wires this; the
    card MUST be taken from the hook's ``card`` argument, not ``reviewer.card``
    (the reviewer has already advanced by answer time; cf. ``mint.py``).
    """
    global _active_offer

    if ease != 1:
        return
    if not service_client.is_configured(mw.col):
        return

    note = card.note()
    if not _is_mcat_note(note):
        return
    concepts = _concept_tags(note)
    # Nothing to surface without a concept tag AND an explanation to ground on.
    if not concepts or not _explanation_of(note):
        return

    # Anchor the floating banner to the reviewer web view.
    anchor: QWidget = mw.web
    banner = _TutorOfferBanner(
        anchor,
        concepts[0],
        on_ask=lambda: open_tutor_for_card(mw, card),
    )
    _active_offer = banner
    banner.show_near(anchor)
