# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Client-side grounded generation flow (PRD C1).

The hosted service drafts a grounded, cited MCAT item for a concept; it NEVER
auto-approves and NEVER writes to any collection (see
``backend/supabase/functions/generate/index.ts``). This module is the client
half: it drives ``service_client.generate`` off the UI thread, shows the drafted
item + its citation to a human reviewer, and — only on explicit approval — lands
the (possibly edited) item as a real note via ``add_note`` in a ``CollectionOp``.

Three public entry points are wired by the integrator (``aqt.synapse.__init__``):

* :func:`generate_for_concept` — fetch a draft for a concept and open the review
  dialog.
* :func:`pick_concept_and_generate` — the MANUAL action: choose a concept from
  the collection, then generate.
* :func:`offer_generate_at_mastery` — the MASTERY recommendation: an unobtrusive,
  dismissible offer to generate a tougher item once a concept is mastered (new
  practice belongs after mastery, not after a failure).

Everything degrades cleanly when the service is unconfigured or unreachable: a
tooltip explains the situation and the study loop is never blocked.

Design constraints mirrored from siblings:

* All HTTPS goes through :mod:`aqt.synapse.service_client` (no reimplemented
  HTTP here). The blocking call runs in a :class:`~aqt.operations.QueryOp`.
* The note is built + added on the collection thread in a
  :class:`~aqt.operations.CollectionOp` (see :mod:`aqt.operations.note`), and the
  source lineage is stamped on the generated card's ``custom_data`` exactly like
  :mod:`aqt.synapse.mint`.
"""

from __future__ import annotations

import json
from typing import Any, Literal

import aqt
import aqt.main
from anki.cards import Card
from anki.collection import Collection, OpChangesWithCount
from anki.notes import NoteId
from aqt.operations import CollectionOp, QueryOp
from aqt.qt import *
from aqt.utils import (
    chooseList,
    disable_help_button,
    getOnlyText,
    restoreGeom,
    saveGeom,
    tooltip,
)

from . import service_client
from .provision import (
    MCAT_GROUNDING_FIELD,
    MCAT_NOTETYPE_NAME,
    SYNAPSE_DECK_NAME,
)
from .service_client import ServiceError, ServiceNotConfigured

GEOM_KEY = "synapseGenerateReview"

# Option labels for rendering an MCQ draft's options into the (single) Stem
# field, since the MCAT Application notetype has no separate Options field.
_OPTION_LABELS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"


# --- Draft normalisation -----------------------------------------------------


class _Draft:
    """A service draft normalised into the fields the review dialog needs.

    Built defensively from the (loosely-typed) ``generate`` response so a
    malformed or partial body never crashes the client — missing pieces just
    render empty and the reviewer can edit or reject.
    """

    def __init__(
        self,
        *,
        stem: str,
        answer: str,
        explanation: str,
        grounding: str,
        concept: str,
    ) -> None:
        self.stem = stem
        self.answer = answer
        self.explanation = explanation
        self.grounding = grounding
        self.concept = concept


def _as_str(value: Any) -> str:
    return value if isinstance(value, str) else ""


def _format_citation(citation: Any) -> str:
    """Render the service citation into a readable one-line cited source.

    The service citation shape (index.ts ``Citation``) is
    ``{chunk_id, title, section, anchor, license}``; any subset may be present.
    Falls back to a plain string if the service ever sends one.
    """
    if isinstance(citation, str):
        return citation.strip()
    if not isinstance(citation, dict):
        return ""
    title = _as_str(citation.get("title")).strip()
    section = _as_str(citation.get("section")).strip()
    anchor = _as_str(citation.get("anchor")).strip()
    license_ = _as_str(citation.get("license")).strip()

    main_parts = [p for p in (title, section) if p]
    text = (
        " — ".join(main_parts)
        if main_parts
        else _as_str(citation.get("chunk_id")).strip()
    )
    if anchor:
        text = f"{text} ({anchor})" if text else anchor
    if license_:
        text = f"{text} [{license_}]" if text else license_
    return text


def _stem_with_options(stem: str, options: list[str]) -> str:
    """Compose the stem plus a lettered options block (MCAT has no Options field).

    Only options with text are rendered; an empty option list leaves the stem
    unchanged.
    """
    stem = stem.strip()
    rendered = [
        f"{_OPTION_LABELS[i]}. {opt.strip()}"
        for i, opt in enumerate(options)
        if isinstance(opt, str) and opt.strip() and i < len(_OPTION_LABELS)
    ]
    if not rendered:
        return stem
    options_block = "\n".join(rendered)
    return f"{stem}\n\n{options_block}" if stem else options_block


def _normalise_draft(
    result: dict[str, Any], concept_tag: str, concept_hint: str
) -> _Draft | None:
    """Map a raw ``generate`` response to a :class:`_Draft`, or None if refused.

    The service returns ``{status, item: {stem, options, answerIndex,
    explanation}, citation, concept_tag}`` on success, or a non-"draft" status
    (``refused`` / ``rejected`` / ``error``) with a ``message`` when it declines.
    """
    status = _as_str(result.get("status"))
    item = result.get("item")
    # Anything that isn't a well-formed draft is treated as "no draft"; the
    # caller surfaces the service's message.
    if status not in ("", "draft") or not isinstance(item, dict):
        return None

    stem = _as_str(item.get("stem"))
    options_raw = item.get("options")
    options = options_raw if isinstance(options_raw, list) else []
    explanation = _as_str(item.get("explanation"))

    # Resolve the correct answer from answerIndex when the options are present;
    # fall back to any explicit "answer" the service might send.
    answer = ""
    answer_index = item.get("answerIndex")
    if isinstance(answer_index, int) and 0 <= answer_index < len(options):
        candidate = options[answer_index]
        answer = candidate.strip() if isinstance(candidate, str) else ""
    if not answer:
        answer = _as_str(item.get("answer")).strip()

    # Concept name for the human-facing Concept field: prefer the source note's
    # concept name, else the last segment of the concept tag.
    concept = concept_hint.strip() or _concept_name_from_tag(concept_tag)

    return _Draft(
        stem=_stem_with_options(stem, options),
        answer=answer,
        explanation=explanation.strip(),
        grounding=_format_citation(result.get("citation")),
        concept=concept,
    )


def _concept_name_from_tag(concept_tag: str) -> str:
    """Human-ish concept name from a ``concept::section::id`` tag's last segment."""
    parts = [p for p in concept_tag.split("::") if p]
    if not parts:
        return concept_tag
    return parts[-1].replace("_", " ")


# --- Entry point: generate for a concept -------------------------------------


def generate_for_concept(
    mw: aqt.main.AnkiQt,
    concept_tag: str,
    *,
    source_card: Card | None = None,
) -> None:
    """Fetch a grounded draft for ``concept_tag`` and open the review dialog.

    Runs the blocking service call off the UI thread. On
    ``ServiceNotConfigured`` / ``ServiceError`` (or a service refusal) a tooltip
    explains the situation and nothing else happens — the study loop is never
    blocked.

    ``source_card`` (a missed card) links the approved note back to its source
    via card ``custom_data`` lineage and supplies a concept-name hint.
    """
    concept_tag = concept_tag.strip()
    if not concept_tag:
        tooltip("No concept selected for generation.", parent=mw)
        return

    if not service_client.is_configured(mw.col):
        tooltip("Synapse AI service is not configured.", parent=mw)
        return

    # Capture source lineage now, on the UI thread, so the background op and the
    # approval callback don't touch the Card/Reviewer later.
    source_note_id: NoteId | None = None
    concept_hint = ""
    if source_card is not None:
        source_note = source_card.note()
        source_note_id = source_note.id
        if "Concept" in source_note:
            concept_hint = source_note["Concept"].strip()

    def op(col: Collection) -> dict[str, Any]:
        return service_client.generate(col, concept_tag)

    def on_success(result: dict[str, Any]) -> None:
        draft = _normalise_draft(result, concept_tag, concept_hint)
        if draft is None:
            message = _as_str(result.get("message")) or (
                f"No grounded draft available for {_concept_name_from_tag(concept_tag)}."
            )
            tooltip(message, parent=mw)
            return
        dialog = GenerateReviewDialog(
            mw,
            draft=draft,
            concept_tag=concept_tag,
            source_note_id=source_note_id,
        )
        dialog.show()
        dialog.activateWindow()

    def on_failure(exc: Exception) -> None:
        if isinstance(exc, ServiceNotConfigured):
            tooltip("Synapse AI service is not configured.", parent=mw)
        elif isinstance(exc, ServiceError):
            tooltip(f"Could not generate a draft: {exc}", parent=mw)
        else:
            # Unexpected: surface it rather than swallow.
            raise exc

    QueryOp(parent=mw, op=op, success=on_success).failure(on_failure).with_progress(
        "Generating a grounded item…"
    ).run_in_background()


# --- Entry point: manual concept picker --------------------------------------


def pick_concept_and_generate(mw: aqt.main.AnkiQt) -> None:
    """MANUAL action: pick a concept present in the collection, then generate.

    Offers the concept tags already in the collection as a list; if there are
    none, falls back to a free-text prompt so the user can still target a
    concept the corpus knows about.
    """
    if not service_client.is_configured(mw.col):
        tooltip("Synapse AI service is not configured.", parent=mw)
        return

    tags = _concept_tags(mw.col)
    if tags:
        # Show a friendlier label but keep the machine tag for the request.
        labels = [f"{_concept_name_from_tag(t)}  ({t})" for t in tags]
        row = chooseList(
            "Generate a grounded item for which concept?", labels, parent=mw
        )
        if row < 0 or row >= len(tags):
            return
        concept_tag = tags[row]
    else:
        concept_tag = getOnlyText(
            "Enter a concept tag to generate for "
            "(e.g. concept::BB::1A::control_of_enzyme_activity):",
            parent=mw,
        ).strip()
        if not concept_tag:
            return

    generate_for_concept(mw, concept_tag)


def _concept_tags(col: Collection) -> list[str]:
    """Sorted list of the ``concept::`` tags present in the collection."""
    return sorted(t for t in col.tags.all() if t.startswith("concept::"))


# --- Entry point: miss-time offer --------------------------------------------


def offer_generate_at_mastery(
    mw: aqt.main.AnkiQt,
    card: Card,
    ease: Literal[1, 2, 3, 4],
) -> None:
    """MASTERY recommendation: offer to generate a tougher item once mastered.

    Per learning science (PRD Principle 2 "difficulty must be earned" + B3
    add-then-fade), new generated practice belongs AFTER mastery, not after a
    failure: failed retrieval yields little learning, so piling a fresh item onto
    a miss is the wrong move. Called from ``reviewer_did_answer_card`` (the caller
    passes the hook's ``card``); fires only on an *Easy* answer (``ease == 4``) to
    a now-*mature* MCAT-family card with the service configured. Short-lived,
    dismissible, and never blocks the reviewer. Manual generation stays available
    any time from the reviewer "More" menu / Tools.
    """
    if ease != 4:
        return
    # Only once the card is genuinely mastered (mature: interval >= 21 days).
    if getattr(card, "ivl", 0) < 21:
        return
    if not service_client.is_configured(mw.col):
        return

    note = card.note()
    notetype = note.note_type()
    if notetype is None or not _is_mcat_family(notetype["name"]):
        return

    concept_tag = _concept_tag_of(note)
    if not concept_tag:
        return

    _MissOffer(mw, card=card, concept_tag=concept_tag).show_offer()


def _is_mcat_family(notetype_name: str) -> bool:
    """True for the MCAT Application notetype and the M1 item notetypes.

    Uses the shared "MCAT " prefix so all Synapse application-item notetypes
    (Application, Which-Principle, Data-Snippet, Explain-Why) qualify without
    importing each name.
    """
    return notetype_name == MCAT_NOTETYPE_NAME or notetype_name.startswith("MCAT ")


def _concept_tag_of(note: Any) -> str:
    """The first ``concept::`` tag on a note, or ""."""
    for tag in note.tags:
        if tag.startswith("concept::"):
            return tag
    return ""


class _MissOffer(QWidget):
    """A tiny, non-blocking, dismissible affordance shown at a miss.

    A frameless popup anchored to the main window with a one-line recommendation
    and two buttons (Generate / Dismiss). It auto-closes after a short delay so
    it never lingers in front of the reviewer, and it is purely additive: doing
    nothing simply lets it fade.
    """

    _AUTO_DISMISS_MS = 8000

    def __init__(self, mw: aqt.main.AnkiQt, *, card: Card, concept_tag: str) -> None:
        QWidget.__init__(self, mw, Qt.WindowType.ToolTip)
        self.mw = mw
        self._card = card
        self._concept_tag = concept_tag

        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)

        layout = QHBoxLayout()
        layout.setContentsMargins(12, 8, 12, 8)

        concept_name = _concept_name_from_tag(concept_tag)
        label = QLabel(f"Mastered {concept_name} — generate a tougher item?")
        label.setWordWrap(True)
        layout.addWidget(label)

        generate_btn = QPushButton("Generate")
        qconnect(generate_btn.clicked, self._on_generate)
        layout.addWidget(generate_btn)

        dismiss_btn = QPushButton("Dismiss")
        qconnect(dismiss_btn.clicked, self.close)
        layout.addWidget(dismiss_btn)

        self.setLayout(layout)

    def show_offer(self) -> None:
        self.adjustSize()
        # Anchor near the bottom-centre of the main window, unobtrusively.
        geom = self.mw.geometry()
        size = self.sizeHint()
        x = geom.x() + (geom.width() - size.width()) // 2
        y = geom.y() + geom.height() - size.height() - 60
        self.move(max(geom.x(), x), max(geom.y(), y))
        self.show()
        QTimer.singleShot(self._AUTO_DISMISS_MS, self.close)

    def _on_generate(self) -> None:
        # Capture what we need, then close before kicking off the flow so the
        # popup never overlaps the review dialog.
        card = self._card
        concept_tag = self._concept_tag
        self.close()
        generate_for_concept(self.mw, concept_tag, source_card=card)


# --- Review dialog -----------------------------------------------------------


class GenerateReviewDialog(QDialog):
    """Human review of a grounded draft: Approve / Edit / Reject.

    The drafted stem, answer and explanation are shown in editable fields (so
    "Edit" is simply changing them before approving), and the citation is shown
    prominently — every approved item keeps its grounding (PRD C1). On Approve
    the (possibly edited) draft is written as a real MCAT Application note via
    ``add_note`` in a ``CollectionOp``; Reject/close discards it.
    """

    def __init__(
        self,
        mw: aqt.main.AnkiQt,
        *,
        draft: _Draft,
        concept_tag: str,
        source_note_id: NoteId | None,
    ) -> None:
        QDialog.__init__(self, mw, Qt.WindowType.Window)
        self.mw = mw
        self.mw.garbage_collect_on_dialog_finish(self)
        self._concept_tag = concept_tag
        self._source_note_id = source_note_id

        self.setWindowTitle("Review Generated Item")
        self.setMinimumWidth(520)
        disable_help_button(self)

        self._build_ui(draft)
        restoreGeom(self, GEOM_KEY)

    # --- UI construction -----------------------------------------------------

    def _build_ui(self, draft: _Draft) -> None:
        layout = QVBoxLayout()

        intro = QLabel(
            "Review this AI-drafted item. It is grounded in the cited source "
            "below. Approve to add it to your Synapse deck, edit the fields "
            "first, or reject to discard it."
        )
        intro.setWordWrap(True)
        layout.addWidget(intro)

        form = QFormLayout()

        self._concept_edit = QLineEdit(draft.concept)
        form.addRow("Concept:", self._concept_edit)

        self._stem_edit = QPlainTextEdit(draft.stem)
        self._stem_edit.setMinimumHeight(120)
        form.addRow("Stem:", self._stem_edit)

        self._answer_edit = QLineEdit(draft.answer)
        form.addRow("Answer:", self._answer_edit)

        self._explanation_edit = QPlainTextEdit(draft.explanation)
        self._explanation_edit.setMinimumHeight(90)
        form.addRow("Explanation:", self._explanation_edit)

        layout.addLayout(form)

        # --- Citation (prominent; every approved item keeps its grounding) ---
        citation_box = QGroupBox("Cited source")
        citation_layout = QVBoxLayout()
        self._grounding_edit = QLineEdit(draft.grounding)
        self._grounding_edit.setReadOnly(True)
        citation_layout.addWidget(self._grounding_edit)
        if not draft.grounding:
            warn = QLabel(
                "No citation was returned — this draft is not grounded and "
                "should be rejected."
            )
            warn.setWordWrap(True)
            warn.setEnabled(False)
            citation_layout.addWidget(warn)
        citation_box.setLayout(citation_layout)
        layout.addWidget(citation_box)

        # --- Buttons: Approve / Edit / Reject --------------------------------
        # "Edit" just focuses the stem field; the fields are always editable, so
        # the button is a discoverability affordance rather than a mode switch.
        # Build the buttons as locals (then addButton) so their `.clicked` is
        # connected on a known QPushButton, not addButton's optional return.
        buttons = QDialogButtonBox()

        approve_btn = QPushButton("Approve")
        approve_btn.setDefault(True)
        buttons.addButton(approve_btn, QDialogButtonBox.ButtonRole.AcceptRole)
        qconnect(approve_btn.clicked, self._on_approve)

        edit_btn = QPushButton("Edit")
        buttons.addButton(edit_btn, QDialogButtonBox.ButtonRole.ActionRole)
        qconnect(edit_btn.clicked, lambda: self._stem_edit.setFocus())

        reject_btn = QPushButton("Reject")
        buttons.addButton(reject_btn, QDialogButtonBox.ButtonRole.RejectRole)
        qconnect(reject_btn.clicked, self.reject)

        layout.addWidget(buttons)

        self.setLayout(layout)

    # --- Approve -------------------------------------------------------------

    def _on_approve(self) -> None:
        stem = self._stem_edit.toPlainText().strip()
        answer = self._answer_edit.text().strip()
        explanation = self._explanation_edit.toPlainText().strip()
        grounding = self._grounding_edit.text().strip()
        concept = self._concept_edit.text().strip()

        if not stem or not answer:
            tooltip("A stem and an answer are required to approve.", parent=self)
            return

        mw = self.mw
        concept_tag = self._concept_tag
        source_note_id = self._source_note_id

        def op(col: Collection) -> OpChangesWithCount:
            return _add_generated_note(
                col,
                stem=stem,
                answer=answer,
                explanation=explanation,
                grounding=grounding,
                concept=concept,
                concept_tag=concept_tag,
                source_note_id=source_note_id,
            )

        def on_success(_changes: OpChangesWithCount) -> None:
            tooltip("Added the generated item to Synapse.", parent=mw)

        CollectionOp(parent=mw, op=op).success(on_success).run_in_background()
        self.accept()

    # --- Geometry ------------------------------------------------------------

    def accept(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.accept(self)

    def reject(self) -> None:
        saveGeom(self, GEOM_KEY)
        QDialog.reject(self)


# --- Collection-thread note builder ------------------------------------------


def _add_generated_note(
    col: Collection,
    *,
    stem: str,
    answer: str,
    explanation: str,
    grounding: str,
    concept: str,
    concept_tag: str,
    source_note_id: NoteId | None,
) -> OpChangesWithCount:
    """Build + add an approved item as an MCAT Application note (collection thread).

    Runs entirely on the background collection thread (invoked from a
    ``CollectionOp``), so it must not touch any UI. Fills the notetype's fields
    from the reviewed draft (including Grounding = citation), copies the source
    concept tag, and — when a source card is known — stamps the generated card's
    ``custom_data`` with the lineage, mirroring :mod:`aqt.synapse.mint`.
    """
    notetype = col.models.by_name(MCAT_NOTETYPE_NAME)
    if notetype is None:
        raise RuntimeError(f'notetype "{MCAT_NOTETYPE_NAME}" not found')

    note = col.new_note(notetype)
    # Guard each field: an older/edited notetype may lack a field, and we never
    # want a KeyError to abort a valid approval.
    if "Stem" in note:
        note["Stem"] = stem
    if "Answer" in note:
        note["Answer"] = answer
    if "Explanation" in note:
        note["Explanation"] = explanation
    if "Concept" in note:
        note["Concept"] = concept
    if MCAT_GROUNDING_FIELD in note:
        note[MCAT_GROUNDING_FIELD] = grounding

    # Keep the concept lineage: prefer the source note's concept:: tags; else
    # the concept tag this item was generated for.
    tags = _lineage_tags(col, concept_tag, source_note_id)
    if tags:
        note.tags = tags

    deck_id = col.decks.id(SYNAPSE_DECK_NAME)
    assert deck_id is not None  # id() creates the deck when missing
    changes = col.add_note(note, deck_id)

    # Stamp source lineage on the generated card(s), mirroring mint.py. Only when
    # this item came from a miss (a source note is known).
    if source_note_id is not None:
        for card in note.cards():
            card.custom_data = json.dumps(
                {"src": int(source_note_id)}, separators=(",", ":")
            )
            col.update_card(card)

    return changes


def _lineage_tags(
    col: Collection,
    concept_tag: str,
    source_note_id: NoteId | None,
) -> list[str]:
    """Concept tags to attach to the generated note.

    Copies the source note's ``concept::`` tags when it came from a miss;
    otherwise attaches the single concept tag it was generated for.
    """
    if source_note_id is not None:
        source = col.get_note(source_note_id)
        copied = [t for t in source.tags if t.startswith("concept::")]
        if copied:
            return copied
    return [concept_tag] if concept_tag.startswith("concept::") else []
