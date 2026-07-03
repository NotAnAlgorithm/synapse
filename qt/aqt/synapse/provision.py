# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Synapse M0 demo-environment provisioning.

This module builds a small, self-contained Synapse demo inside an existing
collection: it turns on FSRS, creates a dedicated "Synapse" deck-config preset
with non-blocked (randomised) ordering, defines an "MCAT Application" notetype,
creates a "Synapse" deck bound to the preset, and seeds a handful of
concept-tagged demo notes.

Design constraint: this file imports ONLY from ``anki.*`` (the pylib). It never
touches ``aqt`` or Qt, takes no GUI objects, and every function operates on an
open ``anki.collection.Collection`` (``col``). That keeps it a pure
collection/backend module which can be loaded directly and exercised headless.

Non-obvious backend behaviours worth calling out:

* FSRS is a *collection-global* boolean. It cannot be flipped with a plain
  config set; it is only persisted through the deck-options update flow
  (``update_deck_configs`` with ``fsrs=True``). See :func:`enable_fsrs_and_preset`.
* In that same flow the *target deck* is (re)bound to whichever config is
  ``configs[-1]``. We exploit that to create the "Synapse" preset (a
  ``DeckConfig`` with ``id == 0`` gets a fresh id assigned by the backend) and
  bind the Synapse deck to it in a single atomic transaction.
* Concept tags follow the convention ``concept::<section>::<id>`` where every
  segment uses underscores and contains no spaces (Anki splits tags on
  whitespace, so a space would create two tags).
"""

from __future__ import annotations

from typing import Any

import anki.collection
from anki import deck_config_pb2
from anki.decks import DeckConfigId, DeckId, UpdateDeckConfigs
from anki.models import NotetypeId

# Public protobuf aliases (kept local so callers needn't reach into `anki`).
DeckConfig = deck_config_pb2.DeckConfig
_Config = DeckConfig.Config

# --- Names / constants -------------------------------------------------------

SYNAPSE_PRESET_NAME = "Synapse"
SYNAPSE_DECK_NAME = "Synapse"
MCAT_NOTETYPE_NAME = "MCAT Application"

# Additional M1 application-item notetypes (decision #4: richer items are new
# notetypes + templates only; no AI grading). See ITEM_NOTETYPES below for their
# fields/templates.
WHICH_PRINCIPLE_NOTETYPE_NAME = "MCAT Which-Principle"
DATA_SNIPPET_NOTETYPE_NAME = "MCAT Data-Snippet"
EXPLAIN_WHY_NOTETYPE_NAME = "MCAT Explain-Why"

# Do NOT lower this; 0.9 is the intended default retention for the demo.
SYNAPSE_DESIRED_RETENTION = 0.9

MCAT_FIELDS = ["Stem", "Passage", "Answer", "Explanation", "Concept"]

# Front: show the passage, then the application-style stem, then a typed answer
# box. `{{type:Answer}}` renders the comparison input against the Answer field.
MCAT_QFMT = """\
<div class="passage">{{Passage}}</div>
<hr class="passage-divider">
<div class="stem">{{Stem}}</div>
{{type:Answer}}"""

# Back: the rendered front, a divider, then the answer and explanation.
MCAT_AFMT = """\
{{FrontSide}}
<hr id="answer">
<div class="answer">{{Answer}}</div>
<div class="explanation">{{Explanation}}</div>"""


# --- M1 application-item notetypes -------------------------------------------
# Three richer application notetypes (decision #4). Each is described purely by
# data here — a display name, its ordered fields, the field to sort on, and its
# single card's front/back templates — and built generically by
# :func:`create_item_notetype`. This mirrors the "MCAT Application" notetype
# above (which keeps its own bespoke constants for backwards compatibility).
#
# Template conventions match the MCAT Application notetype:
#   * `{{type:<Field>}}` renders a typed-answer comparison box where a short,
#     checkable answer fits (Which-Principle, Data-Snippet). Explain-Why has no
#     single correct short answer, so it uses a self-graded reveal instead.
#   * The back always begins with `{{FrontSide}}`, an `<hr id="answer">`, then
#     the reveal.


class ItemNotetypeSpec:
    """Declarative description of an application-item notetype."""

    def __init__(
        self,
        name: str,
        fields: list[str],
        sort_field: str,
        qfmt: str,
        afmt: str,
    ) -> None:
        self.name = name
        self.fields = fields
        self.sort_field = sort_field
        self.qfmt = qfmt
        self.afmt = afmt


ITEM_NOTETYPES: list[ItemNotetypeSpec] = [
    # "Which principle/concept applies?" — a stem plus an options-style prompt,
    # answered with the name of the governing principle.
    ItemNotetypeSpec(
        name=WHICH_PRINCIPLE_NOTETYPE_NAME,
        fields=["Stem", "Options", "Answer", "Explanation", "Concept"],
        sort_field="Stem",
        qfmt="""\
<div class="stem">{{Stem}}</div>
<div class="options">{{Options}}</div>
<div class="prompt">Which principle applies?</div>
{{type:Answer}}""",
        afmt="""\
{{FrontSide}}
<hr id="answer">
<div class="answer">{{Answer}}</div>
<div class="explanation">{{Explanation}}</div>""",
    ),
    # A small data table / figure description followed by a question about it.
    ItemNotetypeSpec(
        name=DATA_SNIPPET_NOTETYPE_NAME,
        fields=["Data", "Question", "Answer", "Explanation", "Concept"],
        sort_field="Question",
        qfmt="""\
<div class="data">{{Data}}</div>
<hr class="data-divider">
<div class="question">{{Question}}</div>
{{type:Answer}}""",
        afmt="""\
{{FrontSide}}
<hr id="answer">
<div class="answer">{{Answer}}</div>
<div class="explanation">{{Explanation}}</div>""",
    ),
    # An open "explain why" prompt whose back reveals a model explanation. No
    # typed-answer box: the learner self-grades against the model answer.
    ItemNotetypeSpec(
        name=EXPLAIN_WHY_NOTETYPE_NAME,
        fields=["Prompt", "ModelAnswer", "Concept"],
        sort_field="Prompt",
        qfmt="""\
<div class="prompt">{{Prompt}}</div>
<div class="hint">Explain your reasoning, then reveal the model answer.</div>""",
        afmt="""\
{{FrontSide}}
<hr id="answer">
<div class="model-answer">{{ModelAnswer}}</div>""",
    ),
]


# --- Seed data ---------------------------------------------------------------
# Edit this list to change the demo content. Each entry is one note. `concept`
# is the human-readable concept name (stored in the Concept field); `tag` is the
# machine concept tag (``concept::<section>::<id>``) attached as the note's tag.
SEED_NOTES: list[dict[str, str]] = [
    # --- Biochem: amino acid charge ---
    {
        "tag": "concept::biochem::amino_acid_charge",
        "concept": "Amino acid charge",
        "stem": "At physiological pH (7.4), what is the net charge of a lysine "
        "side chain in a peptide?",
        "answer": "Positive",
        "explanation": "Lysine's epsilon-amino group has a pKa around 10.5, so "
        "at pH 7.4 it stays protonated and carries a +1 charge.",
    },
    {
        "tag": "concept::biochem::amino_acid_charge",
        "concept": "Amino acid charge",
        "stem": "An enzyme is most active at pH 3. Which residue is most likely "
        "protonated and contributing a positive charge there?",
        "answer": "Histidine",
        "explanation": "Histidine's imidazole (pKa ~6) is protonated below pH 6, "
        "making it the residue whose charge shifts most in acidic conditions.",
    },
    {
        "tag": "concept::biochem::amino_acid_charge",
        "concept": "Amino acid charge",
        "stem": "During isoelectric focusing a protein stops migrating at pH 5.2. "
        "What does this pH represent?",
        "answer": "Its pI",
        "explanation": "At the isoelectric point (pI) the protein's net charge is "
        "zero, so it no longer moves in the electric field.",
    },
    # --- Biochem: enzyme kinetics ---
    {
        "tag": "concept::biochem::enzyme_kinetics",
        "concept": "Enzyme kinetics",
        "stem": "A competitive inhibitor is added to an enzyme assay. How does "
        "the apparent Km change?",
        "answer": "It increases",
        "explanation": "Competitive inhibitors raise the apparent Km (weaker "
        "apparent affinity) while leaving Vmax unchanged.",
    },
    {
        "tag": "concept::biochem::enzyme_kinetics",
        "concept": "Enzyme kinetics",
        "stem": "On a Lineweaver-Burk plot a noncompetitive inhibitor shifts the "
        "y-intercept upward. What does that indicate?",
        "answer": "Lower Vmax",
        "explanation": "The y-intercept equals 1/Vmax, so a higher intercept means "
        "Vmax has decreased, the signature of noncompetitive inhibition.",
    },
    {
        "tag": "concept::biochem::enzyme_kinetics",
        "concept": "Enzyme kinetics",
        "stem": "Substrate concentration is far above Km. What is the approximate "
        "reaction rate relative to Vmax?",
        "answer": "Near Vmax",
        "explanation": "When [S] >> Km the enzyme is saturated, so the rate "
        "approaches Vmax and is nearly independent of substrate concentration.",
    },
    # --- Physics: circuits / Ohm's law ---
    {
        "tag": "concept::physics::circuits_ohms_law",
        "concept": "Ohm's law",
        "stem": "A 12 V battery drives 3 A through a resistor. What is the resistance?",
        "answer": "4 ohms",
        "explanation": "By Ohm's law R = V/I = 12 V / 3 A = 4 ohms.",
    },
    {
        "tag": "concept::physics::circuits_ohms_law",
        "concept": "Ohm's law",
        "stem": "Two identical resistors are placed in parallel. How does the "
        "total resistance compare to one resistor alone?",
        "answer": "Half",
        "explanation": "Equal resistors in parallel combine to R/2, so the total "
        "resistance is half that of a single resistor.",
    },
    {
        "tag": "concept::physics::circuits_ohms_law",
        "concept": "Ohm's law",
        "stem": "A resistor dissipates 6 W while carrying 2 A. What voltage is "
        "across it?",
        "answer": "3 V",
        "explanation": "Power P = IV, so V = P/I = 6 W / 2 A = 3 V.",
    },
    # --- Psych: operant conditioning ---
    {
        "tag": "concept::psych::operant_conditioning",
        "concept": "Operant conditioning",
        "stem": "A rat's lever-pressing increases after a shock is removed each "
        "time it presses. What process is this?",
        "answer": "Negative reinforcement",
        "explanation": "Removing an aversive stimulus (the shock) to increase a "
        "behavior is negative reinforcement.",
    },
    {
        "tag": "concept::psych::operant_conditioning",
        "concept": "Operant conditioning",
        "stem": "A slot machine pays out after an unpredictable number of pulls. "
        "Which reinforcement schedule is this?",
        "answer": "Variable ratio",
        "explanation": "Reward after an unpredictable number of responses is a "
        "variable-ratio schedule, which yields high, steady responding.",
    },
    {
        "tag": "concept::psych::operant_conditioning",
        "concept": "Operant conditioning",
        "stem": "A child stops throwing tantrums after losing screen time for each "
        "outburst. What process reduced the behavior?",
        "answer": "Negative punishment",
        "explanation": "Removing a desirable stimulus (screen time) to decrease a "
        "behavior is negative punishment.",
    },
]


# Demo notes for the M1 item notetypes, keyed by notetype display name. Each
# entry's ``fields`` dict maps field name -> value for that notetype's exact
# field set (see ITEM_NOTETYPES), and ``tag`` is the single concept tag.
# Seeded idempotently by :func:`seed_item_notes`; a couple per type is enough to
# demo the templates without bloating the deck.
ITEM_SEED_NOTES: dict[str, list[dict[str, Any]]] = {
    WHICH_PRINCIPLE_NOTETYPE_NAME: [
        {
            "tag": "concept::biochem::enzyme_kinetics",
            "fields": {
                "Stem": "Doubling [substrate] far below Km roughly doubles the "
                "initial reaction rate.",
                "Options": "First-order kinetics; Zero-order kinetics; "
                "Competitive inhibition; Cooperativity",
                "Answer": "First-order kinetics",
                "Explanation": "Well below Km the rate is approximately "
                "proportional to [S], i.e. first-order in substrate.",
                "Concept": "Enzyme kinetics",
            },
        },
        {
            "tag": "concept::physics::circuits_ohms_law",
            "fields": {
                "Stem": "A wire's current rises linearly as the voltage across it "
                "increases, at constant temperature.",
                "Options": "Ohm's law; Kirchhoff's current law; Faraday's law; "
                "Coulomb's law",
                "Answer": "Ohm's law",
                "Explanation": "A linear V-I relationship at fixed temperature is "
                "the defining behavior of an ohmic resistor (V = IR).",
                "Concept": "Ohm's law",
            },
        },
    ],
    DATA_SNIPPET_NOTETYPE_NAME: [
        {
            "tag": "concept::biochem::enzyme_kinetics",
            "fields": {
                "Data": "Assay | Vmax | Km<br>Control | 100 | 5<br>+Drug X | 100 | 15",
                "Question": "What kind of inhibitor is Drug X?",
                "Answer": "Competitive",
                "Explanation": "Km rises while Vmax is unchanged, the signature of "
                "a competitive inhibitor.",
                "Concept": "Enzyme kinetics",
            },
        },
        {
            "tag": "concept::physics::circuits_ohms_law",
            "fields": {
                "Data": "V (V) | I (A)<br>2 | 0.5<br>4 | 1.0<br>6 | 1.5",
                "Question": "What is the resistance of this component?",
                "Answer": "4 ohms",
                "Explanation": "The V-I ratio is constant at 4 (e.g. 4 V / 1.0 A), "
                "so R = 4 ohms.",
                "Concept": "Ohm's law",
            },
        },
    ],
    EXPLAIN_WHY_NOTETYPE_NAME: [
        {
            "tag": "concept::biochem::amino_acid_charge",
            "fields": {
                "Prompt": "Explain why glycine has no net charge at its pI but "
                "lysine's pI is well above 7.",
                "ModelAnswer": "At the pI the net charge is zero. Glycine has only "
                "its alpha-amino and alpha-carboxyl groups, so its pI sits near "
                "neutral pH. Lysine adds a basic side chain (pKa ~10.5) that must "
                "also be deprotonated to reach net zero, pushing its pI higher.",
                "Concept": "Amino acid charge",
            },
        },
        {
            "tag": "concept::psych::operant_conditioning",
            "fields": {
                "Prompt": "Explain why a variable-ratio schedule produces behavior "
                "that is especially resistant to extinction.",
                "ModelAnswer": "Because reinforcement arrives after an "
                "unpredictable number of responses, the learner cannot tell a "
                "temporary run of non-reward from true extinction, so responding "
                "persists far longer than under predictable schedules.",
                "Concept": "Operant conditioning",
            },
        },
    ],
}


# --- Small utilities ---------------------------------------------------------


def section_of(concept_tag: str) -> str:
    """Return the section, i.e. the 2nd ``::`` segment of a concept tag.

    ``concept::biochem::amino_acid_charge`` -> ``biochem``.
    """
    parts = concept_tag.split("::")
    if len(parts) < 2:
        raise ValueError(f"not a concept tag: {concept_tag!r}")
    return parts[1]


# --- Step 1: FSRS + Synapse preset ------------------------------------------


def enable_fsrs_and_preset(col: anki.collection.Collection) -> DeckConfigId:
    """Enable collection-global FSRS and ensure the "Synapse" preset exists.

    Returns the config id of the "Synapse" preset.

    FSRS is only persisted through the deck-options update flow, so we route
    everything through ``update_deck_configs``. In that flow the *target deck*
    is rebound to ``configs[-1]``; here the target is simply the current deck
    (we only care about the global fsrs flag + creating the preset), and we make
    the preset the last config so nothing else is disturbed. The Synapse deck is
    bound to the preset separately in :func:`create_synapse_deck`, which keeps
    the two concerns independent and each idempotent.
    """
    did = col.decks.get_current_id()
    fu = col.decks.get_deck_configs_for_update(did)

    # Reuse the preset by name if it already exists (idempotent), otherwise
    # start from the backend defaults so all required numeric fields (learn
    # steps, per-day limits, valid FSRS params, ...) are populated. A fresh
    # DeckConfig with id==0 gets a new id assigned by the backend on save.
    preset = _find_config_by_name(fu, SYNAPSE_PRESET_NAME)
    if preset is None:
        preset = DeckConfig()
        preset.id = 0
        preset.name = SYNAPSE_PRESET_NAME
        preset.config.CopyFrom(fu.defaults.config)
    _apply_synapse_config(preset.config)

    # Configs list: send only what we touch. Keep the preset last so, per the
    # backend contract, the target deck ends up on it (harmless here).
    req = UpdateDeckConfigs(
        target_deck_id=did,
        configs=[preset],
        removed_config_ids=[],
        card_state_customizer=fu.card_state_customizer,
        new_cards_ignore_review_limit=fu.new_cards_ignore_review_limit,
        fsrs=True,  # collection-global; only takes effect via this flow
        apply_all_parent_limits=fu.apply_all_parent_limits,
        fsrs_reschedule=False,
        fsrs_health_check=False,
    )
    col.decks.update_deck_configs(req)

    # Re-read to obtain the (possibly newly assigned) preset id.
    return _preset_id(col)


def _apply_synapse_config(config: DeckConfig.Config) -> None:
    """Set the Synapse-specific ordering + retention on an inner Config."""
    # RANDOM gather/sort/review ordering avoids subject-blocking so mixed
    # concepts interleave. Verify enum names against proto/anki/deck_config.proto.
    config.new_card_gather_priority = (
        _Config.NewCardGatherPriority.NEW_CARD_GATHER_PRIORITY_RANDOM_NOTES
    )
    config.new_card_sort_order = (
        _Config.NewCardSortOrder.NEW_CARD_SORT_ORDER_RANDOM_NOTE_THEN_TEMPLATE
    )
    config.review_order = _Config.ReviewCardOrder.REVIEW_CARD_ORDER_RANDOM
    config.desired_retention = SYNAPSE_DESIRED_RETENTION


def _find_config_by_name(fu: Any, name: str) -> DeckConfig | None:
    """Return a *copy* of the named config from a DeckConfigsForUpdate, or None."""
    for cwe in fu.all_config:
        if cwe.config.name == name:
            # Copy so mutations don't touch the shared response object.
            out = DeckConfig()
            out.CopyFrom(cwe.config)
            return out
    return None


def _preset_id(col: anki.collection.Collection) -> DeckConfigId:
    """Look up the Synapse preset id by name (after it has been saved)."""
    fu = col.decks.get_deck_configs_for_update(col.decks.get_current_id())
    for cwe in fu.all_config:
        if cwe.config.name == SYNAPSE_PRESET_NAME:
            return DeckConfigId(cwe.config.id)
    raise RuntimeError("Synapse preset not found after creation")


# --- Step 2: MCAT Application notetype ---------------------------------------


def create_mcat_notetype(col: anki.collection.Collection) -> NotetypeId:
    """Create (or reuse) the "MCAT Application" notetype. Returns its id."""
    existing = col.models.by_name(MCAT_NOTETYPE_NAME)
    if existing is not None:
        return NotetypeId(existing["id"])

    nt = col.models.new(MCAT_NOTETYPE_NAME)
    for field_name in MCAT_FIELDS:
        col.models.add_field(nt, col.models.new_field(field_name))

    # Sort on Stem (the human-facing question) if present.
    if "Stem" in MCAT_FIELDS:
        col.models.set_sort_index(nt, MCAT_FIELDS.index("Stem"))

    template = col.models.new_template("Card 1")
    template["qfmt"] = MCAT_QFMT
    template["afmt"] = MCAT_AFMT
    col.models.add_template(nt, template)

    out = col.models.add_dict(nt)
    return NotetypeId(out.id)


def create_item_notetype(
    col: anki.collection.Collection, spec: ItemNotetypeSpec
) -> NotetypeId:
    """Create (or reuse) an application-item notetype from its spec.

    Idempotent: if a notetype with the spec's name already exists we return its
    id untouched (we never mutate an existing notetype's fields/templates, which
    would be a destructive schema change for any notes already using it).
    """
    existing = col.models.by_name(spec.name)
    if existing is not None:
        return NotetypeId(existing["id"])

    nt = col.models.new(spec.name)
    for field_name in spec.fields:
        col.models.add_field(nt, col.models.new_field(field_name))

    # Sort on the human-facing prompt field.
    if spec.sort_field in spec.fields:
        col.models.set_sort_index(nt, spec.fields.index(spec.sort_field))

    template = col.models.new_template("Card 1")
    template["qfmt"] = spec.qfmt
    template["afmt"] = spec.afmt
    col.models.add_template(nt, template)

    out = col.models.add_dict(nt)
    return NotetypeId(out.id)


def create_item_notetypes(
    col: anki.collection.Collection,
) -> dict[str, NotetypeId]:
    """Create (or reuse) all M1 item notetypes. Returns name -> id."""
    return {spec.name: create_item_notetype(col, spec) for spec in ITEM_NOTETYPES}


# --- Step 3: Synapse deck ----------------------------------------------------


def create_synapse_deck(
    col: anki.collection.Collection, preset_id: DeckConfigId
) -> DeckId:
    """Create (or reuse) the "Synapse" deck and bind it to the preset."""
    did = col.decks.id(SYNAPSE_DECK_NAME)
    assert did is not None  # id() creates when missing

    # Bind the deck to the Synapse preset (idempotent: set_config_id just
    # rewrites the deck dict's "conf").
    deck = col.decks.get(did)
    assert deck is not None
    if deck.get("conf") != preset_id:
        col.decks.set_config_id_for_deck_dict(deck, preset_id)

    return did


# --- Step 4: seed notes ------------------------------------------------------


def seed_notes(
    col: anki.collection.Collection,
    notetype_id: NotetypeId,
    deck_id: DeckId,
) -> int:
    """Seed the demo MCAT notes. Idempotent. Returns the number added.

    Guard: if the Synapse deck already contains any MCAT Application notes we
    skip seeding entirely rather than risk duplicates.
    """
    if col.find_notes(f'deck:{SYNAPSE_DECK_NAME} note:"{MCAT_NOTETYPE_NAME}"'):
        return 0

    notetype = col.models.get(notetype_id)
    assert notetype is not None

    added = 0
    for spec in SEED_NOTES:
        note = col.new_note(notetype)
        note["Stem"] = spec["stem"]
        note["Passage"] = spec.get("passage", "")
        note["Answer"] = spec["answer"]
        note["Explanation"] = spec["explanation"]
        note["Concept"] = spec["concept"]
        # Exactly one concept tag per note; segments are underscore-joined and
        # space-free so Anki treats it as a single tag.
        note.tags = [spec["tag"]]
        col.add_note(note, deck_id)
        added += 1
    return added


def seed_item_notes(
    col: anki.collection.Collection,
    notetype_ids: dict[str, NotetypeId],
    deck_id: DeckId,
) -> int:
    """Seed demo notes for the M1 item notetypes. Idempotent. Returns count.

    Per notetype: skip seeding if the Synapse deck already holds any note of
    that notetype (so re-provisioning never duplicates). Each seed spec's
    ``fields`` dict must cover exactly that notetype's fields.
    """
    added = 0
    for name, specs in ITEM_SEED_NOTES.items():
        notetype_id = notetype_ids.get(name)
        if notetype_id is None:
            continue
        # Per-notetype guard so a partially-seeded deck still fills the rest.
        if col.find_notes(f'deck:{SYNAPSE_DECK_NAME} note:"{name}"'):
            continue

        notetype = col.models.get(notetype_id)
        assert notetype is not None
        # Field set to write into (the notetype's declared fields).
        field_names = {fld["name"] for fld in notetype["flds"]}

        for spec in specs:
            note = col.new_note(notetype)
            for field_name, value in spec["fields"].items():
                if field_name in field_names:
                    note[field_name] = value
            # Exactly one concept tag per note; underscore-joined + space-free
            # so Anki treats it as a single tag.
            note.tags = [spec["tag"]]
            col.add_note(note, deck_id)
            added += 1
    return added


def is_provisioned(col: anki.collection.Collection) -> bool:
    """True once the Synapse environment exists (gates auto-provision).

    Presence of the MCAT Application notetype is our provisioned marker.
    """
    return col.models.by_name(MCAT_NOTETYPE_NAME) is not None


# --- Orchestration -----------------------------------------------------------


def provision(col: anki.collection.Collection) -> dict[str, Any]:
    """Provision the full Synapse demo environment. Idempotent.

    Returns a summary dict with the created/looked-up ids and counts.
    """
    config_id = enable_fsrs_and_preset(col)
    notetype_id = create_mcat_notetype(col)
    item_notetype_ids = create_item_notetypes(col)
    deck_id = create_synapse_deck(col, config_id)
    notes_added = seed_notes(col, notetype_id, deck_id)
    item_notes_added = seed_item_notes(col, item_notetype_ids, deck_id)

    return {
        "notetype_id": int(notetype_id),
        "item_notetype_ids": {
            name: int(nid) for name, nid in item_notetype_ids.items()
        },
        "deck_id": int(deck_id),
        "config_id": int(config_id),
        "notes_added": notes_added,
        "item_notes_added": item_notes_added,
        "fsrs": True,
    }
