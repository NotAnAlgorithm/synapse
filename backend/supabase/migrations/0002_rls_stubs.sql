-- Synapse service — per-user tables + RLS policy STUBS.
--
-- These are the three per-user data stores the service keeps server-side and
-- keys by user (M2 design §5.5): tutor threads (C2), placement sessions (D3),
-- and F3 calibration tuples. The COLLECTION itself never lives here — only
-- derived / AI artifacts (M2 design §3, §5.5).
--
-- IMPORTANT: the owner-facing auth fork is still open (M2 design §3, §10):
--   Option A  reuse AnkiWeb-style sync accounts, OR
--   Option B  a dedicated Synapse identity linked to sync (design recommendation).
-- Until that is decided we do NOT know whether `user_id` is `auth.uid()` (native
-- Supabase Auth) or an externally-minted Synapse id validated by an Edge
-- Function. So the tables are defined, RLS is ENABLED (deny-by-default: with no
-- permissive policy, non-service access is refused), and the actual per-user
-- USING/WITH CHECK predicates are left COMMENTED for the owner to un-stub once
-- the auth model is chosen. The service role bypasses RLS and is what the Edge
-- Functions use in the interim (M2 design §3: dev-token interim).

-- ---------------------------------------------------------------------------
-- tutor_threads (C2): the state-grounded tutor's per-user dialogue history.
-- Kept server-side so the tutor can be grounded in prior turns; never synced
-- to the collection (M2 design §5.5).
-- ---------------------------------------------------------------------------
create table if not exists public.tutor_threads (
    id           uuid primary key default gen_random_uuid(),
    user_id      text        not null,   -- opaque Synapse/Auth user id (TBD §3)
    concept_tag  text,                   -- concept the miss was on, if any
    card_id      bigint,                 -- client-supplied card id (not a FK here)
    messages     jsonb       not null default '[]'::jsonb,  -- ordered turns
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);
create index if not exists tutor_threads_user_idx
    on public.tutor_threads (user_id);

alter table public.tutor_threads enable row level security;

-- STUB — un-comment and pick the predicate that matches the chosen auth model.
-- Option B (native Supabase Auth, user_id = auth.uid()::text):
--   create policy tutor_threads_owner on public.tutor_threads
--     for all
--     using (user_id = auth.uid()::text)
--     with check (user_id = auth.uid()::text);
-- Option A / external Synapse identity: enforce user scoping inside the Edge
--   Function (which derives user_id from the validated bearer token) and keep
--   direct client access denied by leaving RLS on with no permissive policy.

-- ---------------------------------------------------------------------------
-- placement_sessions (D3): IRT placement runs + resulting per-concept credit.
-- Only the *result* (mastery credit) lands in the core via the client; the
-- session/IRT state stays here (M2 design §5.2, §5.5).
-- ---------------------------------------------------------------------------
create table if not exists public.placement_sessions (
    id             uuid primary key default gen_random_uuid(),
    user_id        text        not null,
    -- Running IRT state: ability estimate (theta), its standard error, and the
    -- administered/answered items. Shape is the placement engine's concern.
    irt_state      jsonb       not null default '{}'::jsonb,
    -- Per-concept results: [{concept_tag, credit_level in confirmed|partial|none}]
    -- Never credit mastery on a single correct answer (PRD D3/D5).
    concept_credits jsonb      not null default '[]'::jsonb,
    status         text        not null default 'in_progress',  -- in_progress|complete
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now()
);
create index if not exists placement_sessions_user_idx
    on public.placement_sessions (user_id);

alter table public.placement_sessions enable row level security;

-- STUB — same fork as above.
-- create policy placement_sessions_owner on public.placement_sessions
--   for all
--   using (user_id = auth.uid()::text)
--   with check (user_id = auth.uid()::text);

-- ---------------------------------------------------------------------------
-- calibration_tuples (F3): consented, de-identified predicted-vs-actual tuples.
-- The ONLY cross-user aggregation; minimal payload, no collection contents
-- (M2 design §5.6). Consent + privacy policy is an owner decision (§10).
-- ---------------------------------------------------------------------------
create table if not exists public.calibration_tuples (
    id               uuid primary key default gen_random_uuid(),
    -- Opaque, de-identified subject id — NOT joined to any collection data.
    subject_id       text        not null,
    predicted_low    integer,                 -- predicted score range, low
    predicted_high   integer,                 -- predicted score range, high
    actual_score     integer,                 -- reported real AAMC score
    coverage         real,                    -- fraction of concepts covered
    features         jsonb       not null default '{}'::jsonb,  -- model features
    consented_at     timestamptz not null default now(),
    created_at       timestamptz not null default now()
);
create index if not exists calibration_tuples_subject_idx
    on public.calibration_tuples (subject_id);

alter table public.calibration_tuples enable row level security;

-- STUB — F3 is de-identified aggregate data; the exact access rule (typically
-- insert-only from the consented client, read only by the service role for
-- refits) depends on the consent/privacy policy the owner sets (§5.6, §10).
-- create policy calibration_tuples_insert_own on public.calibration_tuples
--   for insert
--   with check (subject_id = auth.uid()::text);
