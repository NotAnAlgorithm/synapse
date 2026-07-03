// Rule-based item-flaw checker — DETERMINISTIC, not an LLM judge.
//
// This is the automated quality gate that runs AFTER the grounding check and
// BEFORE any human sees a draft (M2 design §4.3). It encodes classic
// item-writing rules so reviewers spend time on judgment, not on catching
// mechanical defects. Per the PRD (C1), a rule-based checker catches ~91% of
// item-writing flaws vs ~79% for an LLM judge, and AI MCQs carry higher rates of
// exactly the defects below — which is why this is code, not a model call.
//
// A structural defect is a HARD FAIL: the draft is not queued for human review.

/** A multiple-choice item in the checker's neutral shape. */
export interface McqDraft {
  stem: string;
  options: string[];
  /** 0-based index of the single intended-correct option. */
  answerIndex: number;
  explanation?: string;
}

export interface FlawFinding {
  rule: string;
  message: string;
}

export interface FlawResult {
  ok: boolean;
  findings: FlawFinding[];
}

// Options that abuse the "combined option" pattern; classic item-writing flaws.
const ALL_OF_THE_ABOVE = /\ball of the above\b/i;
const NONE_OF_THE_ABOVE = /\bnone of the above\b/i;
// Absolute terms in options are a common answer-giveaway / grammatical cue.
const ABSOLUTE_TERM = /\b(always|never|all|none|every|only|must)\b/i;

function normalize(s: string): string {
  return s.trim().toLowerCase().replace(/\s+/g, " ");
}

/**
 * Run the deterministic item-writing rules. Returns ok=false with findings if
 * any structural defect is present. Rules mirror M2 design §4.3.
 */
export function checkMcq(draft: McqDraft): FlawResult {
  const findings: FlawFinding[] = [];
  const opts = draft.options ?? [];

  // R1 — single clear stem: the stem must be present and non-trivial.
  if (!draft.stem || draft.stem.trim().length < 3) {
    findings.push({ rule: "single-clear-stem", message: "Stem is empty or too short." });
  }

  // R2 — enough options to be a real MCQ (>=3 keeps a plausible distractor set).
  if (opts.length < 3) {
    findings.push({
      rule: "option-count",
      message: `Item has ${opts.length} option(s); need at least 3.`,
    });
  }

  // R3 — valid, in-range answer key pointing at a single option.
  if (
    !Number.isInteger(draft.answerIndex) ||
    draft.answerIndex < 0 ||
    draft.answerIndex >= opts.length
  ) {
    findings.push({
      rule: "answer-key",
      message: "answerIndex does not point at a valid option.",
    });
  }

  // R4 — no duplicate / defensibly-equivalent options (a source of
  // multiple-correct-answer defects, ~4-5% in AI MCQs per PRD C1).
  const seen = new Map<string, number>();
  opts.forEach((opt, i) => {
    const key = normalize(opt);
    if (key.length === 0) {
      findings.push({ rule: "empty-option", message: `Option ${i + 1} is empty.` });
      return;
    }
    if (seen.has(key)) {
      findings.push({
        rule: "duplicate-option",
        message: `Options ${seen.get(key)! + 1} and ${i + 1} are equivalent ` +
          `(risk of multiple defensibly-correct answers).`,
      });
    } else {
      seen.set(key, i);
    }
  });

  // R5 — no "all/none of the above" abuse.
  opts.forEach((opt, i) => {
    if (ALL_OF_THE_ABOVE.test(opt) || NONE_OF_THE_ABOVE.test(opt)) {
      findings.push({
        rule: "all-none-of-the-above",
        message: `Option ${i + 1} uses an "all/none of the above" construction.`,
      });
    }
  });

  // R6 — answer-giveaway by length: if the correct option is much longer than
  // every distractor, length itself cues the answer.
  if (draft.answerIndex >= 0 && draft.answerIndex < opts.length) {
    const correctLen = opts[draft.answerIndex].trim().length;
    const others = opts.filter((_, i) => i !== draft.answerIndex);
    const maxOther = others.reduce((m, o) => Math.max(m, o.trim().length), 0);
    if (others.length > 0 && correctLen > 0 && correctLen >= 2 * (maxOther || 1)) {
      findings.push({
        rule: "answer-length-cue",
        message: "Correct option is far longer than the distractors (length cue).",
      });
    }
  }

  // R7 — grammatical / absolute-term cue: absolute qualifiers in ONLY some
  // options are a classic giveaway. Flag when they appear on the key alone.
  if (draft.answerIndex >= 0 && draft.answerIndex < opts.length) {
    const keyHasAbsolute = ABSOLUTE_TERM.test(opts[draft.answerIndex]);
    const anyDistractorAbsolute = opts.some(
      (o, i) => i !== draft.answerIndex && ABSOLUTE_TERM.test(o),
    );
    if (keyHasAbsolute && !anyDistractorAbsolute) {
      findings.push({
        rule: "absolute-term-cue",
        message: "Only the correct option uses an absolute term (grammatical cue).",
      });
    }
  }

  return { ok: findings.length === 0, findings };
}
