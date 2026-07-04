# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""State-grounded Socratic tutor (PRD C2).

When a student answers an MCAT-family item *Again* (ease 1), we treat the miss
as a knowledge gap and *proactively but unobtrusively* offer a Socratic tutor
grounded in (a) the item's verified ``Explanation`` and (b) the student's
per-concept mastery map. The tutor never reveals the answer; it surfaces the
weak *prerequisite* behind the mistake (see notes/M3_tutor_design.md §1, §3, §4).

Design constraints (M2 §1-2, M3 §5.1 "degrade cleanly"):

* The Rust core makes **no** network calls. The *client* assembles the
  student-state bundle from local read-models (``concept_mastery`` RPC + the
  note's ``Explanation`` field) and POSTs it to the hosted service over HTTPS
  via :mod:`aqt.synapse.service_client`.
* The base study loop **never depends on the tutor**. The affordance is
  dismissible and non-blocking; if the service is not configured or a call
  fails, we degrade to a quiet "tutor unavailable" state and the review
  continues (the mint path from the same miss is 100% local and unaffected).
* All HTTPS is BLOCKING, so both the bundle assembly (a local RPC read) and the
  tutor call run off the UI thread through ``QueryOp``.

Public entry points (wired by ``aqt.synapse.__init__`` — the integrator owns
all menu/hook wiring; this module never touches ``__init__`` or the hooks):

* :func:`open_tutor_for_card` ``(mw, card) -> None`` — assemble the bundle for a
  card and open the tutor panel with the first turn.
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


# --- Bundle assembly (pure, runs on the collection thread) -------------------


def _is_mcat_note(note: Note) -> bool:
    notetype = note.note_type()
    return notetype is not None and notetype["name"] in _MCAT_NOTETYPES


def _concept_tags(note: Note) -> list[str]:
    """The note's ``concept::<section>::<id>`` tags (may be several; may be empty)."""
    return [t for t in note.tags if t.startswith("concept::")]


def _explanation_of(note: Note) -> str:
    """The item's verified explanation -- the tutor's answer-side grounding.

    Prefers the ``Explanation`` field (MCAT Application + the typed-answer item
    notetypes); falls back to ``ModelAnswer`` (Explain-Why). Empty when neither
    is present -- the tutor then has nothing to ground on and is skipped.
    """
    for field in ("Explanation", "ModelAnswer"):
        if field in note:
            text = note[field].strip()
            if text:
                return text
    return ""


def _answer_of(note: Note) -> str:
    """The item's short answer, for the SERVER's giveaway post-check ONLY.

    Never used to prompt the model (the tutor is grounded in the explanation,
    not the answer); passed only so the service can reject a turn that echoes it.
    """
    if "Answer" in note:
        return note["Answer"].strip()
    return ""


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


def _assemble_bundle(col: Collection, note_id: NoteId) -> dict[str, Any] | None:
    """Collection-thread body: build the ``{concept, item_explanation, ...}`` payload.

    Re-fetches the note by id on the collection thread (matching ``mint.py``'s
    discipline of not carrying a UI-thread note into the background op). Returns
    ``None`` when there is nothing to ground on (no concept tag, or no
    explanation text) so the caller can degrade quietly instead of firing an
    ungroundable request.
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
        "answer": _answer_of(note),
        "mastery_bundle": _bundle_for_concept(primary) if primary is not None else {},
    }


# --- Turn extraction ---------------------------------------------------------


def _turns_from_response(data: dict[str, Any]) -> list[str]:
    """Pull assistant turn text(s) out of the tutor response.

    Accepts the primary ``{turns: [{role, content}, ...]}`` shape and the
    alternative ``{reply: "..."}`` shape (M3 §3.1 / task spec)."""
    turns = data.get("turns")
    texts: list[str] = []
    if isinstance(turns, list):
        for turn in turns:
            if isinstance(turn, dict):
                content = turn.get("content")
                if isinstance(content, str) and content.strip():
                    texts.append(content.strip())
            elif isinstance(turn, str) and turn.strip():
                texts.append(turn.strip())
    reply = data.get("reply")
    if not texts and isinstance(reply, str) and reply.strip():
        texts.append(reply.strip())
    return texts


# --- The tutor panel (read-only dialogue) ------------------------------------


class SynapseTutorDialog(QDialog):
    """A native, read-only panel showing the tutor's Socratic turn(s).

    Deliberately a plain ``QDialog`` (not a webview): the turns are supplied by
    the caller after the off-thread service call, so no in-panel API access is
    needed. A follow-up input could be added later; M3 ships read-only (§4).
    """

    def __init__(
        self,
        mw: aqt.main.AnkiQt,
        concept: str,
        turns: list[str],
        surfaced_prerequisite: str | None = None,
    ) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        self.mw = mw
        mw.garbage_collect_on_dialog_finish(self)
        self.setWindowTitle("Synapse Tutor")
        self.setMinimumWidth(460)
        disable_help_button(self)
        self._build_ui(concept, turns, surfaced_prerequisite)
        restoreGeom(self, GEOM_KEY)

    def _build_ui(
        self, concept: str, turns: list[str], surfaced_prerequisite: str | None
    ) -> None:
        layout = QVBoxLayout()

        heading = QLabel(f"Tutor — {_concept_label(concept)}")
        font = heading.font()
        font.setBold(True)
        heading.setFont(font)
        layout.addWidget(heading)

        if surfaced_prerequisite:
            hint = QLabel(f"Likely gap: {_concept_label(surfaced_prerequisite)}")
            hint.setWordWrap(True)
            hint.setEnabled(False)  # muted, description-style
            layout.addWidget(hint)

        body = QTextBrowser()
        body.setOpenExternalLinks(False)
        body.setPlainText("\n\n".join(turns))
        body.setMinimumHeight(200)
        layout.addWidget(body)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)  # type: ignore[arg-type]
        qconnect(buttons.rejected, self.reject)
        layout.addWidget(buttons)

        self.setLayout(layout)

    def reject(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.reject(self)


def _concept_label(tag: str) -> str:
    """Human-readable leaf of a ``concept::<section>::<id>`` tag."""
    parts = [p for p in tag.split("::") if p]
    leaf = parts[-1] if parts else tag
    return leaf.replace("_", " ").strip() or tag


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


def open_tutor_for_card(mw: aqt.main.AnkiQt, card: Card) -> None:
    """Assemble the state bundle for ``card`` and open the tutor panel.

    Runs the (local) bundle assembly and the (network) tutor call off the UI
    thread via ``QueryOp``. Degrades quietly if the service is off or errors.
    """
    if not service_client.is_configured(mw.col):
        tooltip("Synapse tutor is not configured", parent=mw)
        return

    note = card.note()
    if not _is_mcat_note(note):
        return
    note_id = note.id

    def op(col: Collection) -> dict[str, Any] | None:
        # Assemble the bundle (local RPC read) then call the tutor (HTTPS). Both
        # are off the UI thread here.
        bundle = _assemble_bundle(col, note_id)
        if bundle is None:
            return None
        response = service_client.tutor_turn(col, bundle)
        # Carry the concept through for the panel heading.
        response["_concept"] = bundle["concept"]
        return response

    def on_success(response: dict[str, Any] | None) -> None:
        if response is None:
            tooltip("Nothing for the tutor to work with here", parent=mw)
            return
        concept = str(response.get("_concept", ""))
        turns = _turns_from_response(response)
        if not turns:
            tooltip("The tutor had no guidance to offer", parent=mw)
            return
        surfaced = response.get("surfaced_prerequisite")
        SynapseTutorDialog(
            mw,
            concept,
            turns,
            surfaced if isinstance(surfaced, str) else None,
        ).show()

    def on_failure(exc: Exception) -> None:
        # Best-effort: the base loop never depends on the tutor (M3 §5.1).
        if isinstance(exc, service_client.ServiceNotConfigured):
            tooltip("Synapse tutor is not configured", parent=mw)
        else:
            tooltip("Tutor unavailable right now", parent=mw)

    QueryOp(parent=mw, op=op, success=on_success).failure(on_failure).with_progress(
        "Asking the Synapse tutor..."
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
