# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Error-driven card minting (PRD B1).

The flagship Synapse interaction: when a student answers an "MCAT Application"
card *Again* (ease 1), we treat that as a knowledge gap and let them mint a
focused recall card from it, linked back to the source note.

Design notes:

* We never edit ``reviewer.py``. Misses are detected by appending to the
  ``gui_hooks.reviewer_did_answer_card`` hook (signature
  ``(reviewer, card, ease)`` per ``out/qt/_aqt/hooks.py`` around line 4176).
  The stashed card is captured from the hook's ``card`` argument -- not
  ``reviewer.card`` -- because by answer time the reviewer has already advanced.
* The mint action is bound to ``Ctrl+M`` via
  ``gui_hooks.state_shortcuts_will_change`` (signature
  ``(state, shortcuts)`` per ``out/qt/_aqt/hooks.py`` around line 4971).
  ``reviewer_will_init_shortcuts`` does NOT exist in this tree, so this is the
  correct additive hook: it fires from ``AnkiQt.setStateShortcuts`` for every
  state, and we only append our binding while ``state == "review"``.
* Minting runs off the UI thread through ``aqt.operations.CollectionOp``: it
  builds a Basic note, adds it to the Synapse deck, then stamps the generated
  card's ``custom_data`` with the source lineage and persists it. ``custom_data``
  is a JSON string on the ``Card`` wrapper (``pylib/anki/cards.py``); it is
  capped ~100 bytes with keys <=8 bytes, so ``{"src": <int>}`` fits comfortably.
"""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Literal

import aqt
import aqt.main
import aqt.reviewer
from anki.cards import Card
from anki.collection import Collection, OpChangesWithCount
from anki.notes import Note, NoteId
from aqt import gui_hooks
from aqt.operations import CollectionOp
from aqt.utils import tooltip

from .provision import MCAT_NOTETYPE_NAME, SYNAPSE_DECK_NAME

# Fields on the built-in "Basic" notetype used for minted recall cards.
_BASIC_NOTETYPE_NAME = "Basic"
_BASIC_FRONT = "Front"
_BASIC_BACK = "Back"

# Shortcut that triggers a mint from the stashed missed card.
_MINT_SHORTCUT = "Ctrl+M"

# The single most-recently-missed MCAT note id, stashed by the answer hook. We
# only ever need the last miss, so a module-level slot is sufficient (and avoids
# leaking a growing list). ``None`` means "nothing to mint".
_last_missed_note_id: NoteId | None = None


def _on_answer(
    reviewer: aqt.reviewer.Reviewer,
    card: Card,
    ease: Literal[1, 2, 3, 4],
) -> None:
    """Stash the note of a missed ("Again") MCAT Application card."""
    global _last_missed_note_id
    del reviewer  # required by the hook signature, unused here

    if ease != 1:
        return

    note = card.note()
    notetype = note.note_type()
    if notetype is None or notetype["name"] != MCAT_NOTETYPE_NAME:
        return

    _last_missed_note_id = note.id
    tooltip(f"Missed - press {_MINT_SHORTCUT} to mint a recall card")


def _on_shortcuts(
    state: aqt.main.MainWindowState,
    shortcuts: list[tuple[str, Callable]],
) -> None:
    """While reviewing, bind Ctrl+M to mint from the last missed card."""
    if state != "review":
        return
    shortcuts.append((_MINT_SHORTCUT, _mint_last_missed))


def _mint_last_missed() -> None:
    """Mint a recall card from the stashed missed note, if any."""
    mw = aqt.mw
    assert mw is not None

    if _last_missed_note_id is None:
        tooltip("Nothing to mint")
        return

    source_note_id = _last_missed_note_id

    def on_success(_changes: OpChangesWithCount) -> None:
        global _last_missed_note_id
        # Consume the stash so a repeated Ctrl+M doesn't duplicate the card.
        _last_missed_note_id = None
        tooltip("Minted a recall card in Synapse")

    CollectionOp(parent=mw, op=lambda col: _mint(col, source_note_id)).success(
        on_success
    ).run_in_background()


def _mint(col: Collection, source_note_id: NoteId) -> OpChangesWithCount:
    """Collection-thread body: build+add the recall note, then link its card.

    Runs entirely on the background collection thread (invoked from a
    ``CollectionOp``), so it must not touch any UI.
    """
    source = col.get_note(source_note_id)

    basic = col.models.by_name(_BASIC_NOTETYPE_NAME)
    if basic is None:
        raise RuntimeError(f'notetype "{_BASIC_NOTETYPE_NAME}" not found')

    note = col.new_note(basic)
    note[_BASIC_FRONT] = _front_prompt(source)
    note[_BASIC_BACK] = _back_answer(source)
    # Copy the source's concept tags so the minted card stays in the same
    # concept lineage for the memory graph.
    note.tags = [t for t in source.tags if t.startswith("concept::")]

    deck_id = col.decks.id(SYNAPSE_DECK_NAME)
    assert deck_id is not None  # id() creates the deck when missing
    changes = col.add_note(note, deck_id)

    # Stamp lineage on the generated card. Basic yields exactly one card.
    for card in note.cards():
        card.custom_data = json.dumps({"src": source.id}, separators=(",", ":"))
        col.update_card(card)

    return changes


def _front_prompt(source: Note) -> str:
    """A focused recall prompt derived from the source note."""
    stem = source["Stem"].strip() if "Stem" in source else ""
    if stem:
        return stem
    concept = source["Concept"].strip() if "Concept" in source else ""
    return f"Recall: {concept}" if concept else "Recall"


def _back_answer(source: Note) -> str:
    """The answer + explanation from the source note."""
    answer = source["Answer"].strip() if "Answer" in source else ""
    explanation = source["Explanation"].strip() if "Explanation" in source else ""
    parts = [p for p in (answer, explanation) if p]
    return "<br><br>".join(parts)


def install_hooks() -> None:
    """Wire the mint hooks. Call once at startup."""
    gui_hooks.reviewer_did_answer_card.append(_on_answer)
    gui_hooks.state_shortcuts_will_change.append(_on_shortcuts)
