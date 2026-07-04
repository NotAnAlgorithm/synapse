# Synapse — cloud progress sync + login (self-hosted)

**Decision (owner, 2026-07-04):** identity = a **self-hosted Anki sync server**.
That server's account is the login; your whole collection (cards, reviews, FSRS
state, concept tags, Synapse config) syncs to it natively over Anki's sync
protocol. The AI service (Supabase) is separate and, for now, accepts the
interim service token; a later step can exchange the sync session for a service
token so it's one login (M2 design §3).

This is almost entirely **native Anki** — Synapse only points the client at your
server and brands the flow. Nothing about sync depends on the AI service, and
the base study loop works with no server at all.

---

## 1. Run the sync server (your infra)

The sync server ships inside the core. Two ways to run it:

**A. Directly (built-in module).** With a built pylib on `PYTHONPATH`:

```bash
export SYNC_USER1="you@example.com:a-strong-password"   # repeat SYNC_USER2=... for more users
export SYNC_HOST="0.0.0.0"
export SYNC_PORT="8080"
# export SYNC_BASE="/path/to/persistent/data"           # where collections are stored
python -m anki.syncserver
```

**B. Docker (recommended for a real deployment).** See `docs/syncserver/`
(`Dockerfile` / `Dockerfile.distroless`) — build once, then:

```bash
docker run -d -e "SYNC_USER1=you@example.com:a-strong-password" \
  -p 8080:8080 \
  --mount type=volume,src=synapse-sync-data,dst=/anki_data \
  --name synapse-sync anki-sync-server
```

Full env-var reference: <https://docs.ankiweb.net/sync-server.html> (`SYNC_USER1`,
`SYNC_HOST`, `SYNC_PORT`, `SYNC_BASE`; in Docker `SYNC_BASE`/`SYNC_PORT` are
managed for you). Passwords are set here — **this account list IS the identity**.

**Production:** put it behind HTTPS (a reverse proxy — Caddy/nginx/Cloudflare) so
the client can use an `https://` URL; sync payloads include your collection.

---

## 2. Point Synapse at it (the client)

Either:

- **Synapse ▸ Service & Sync settings…** → *Sync server URL* → e.g.
  `https://sync.your-host.example/` (this writes Anki's custom sync URL), or
- **Preferences ▸ Syncing ▸ Self-hosted sync server** (the native field).

Then **File ▸ Sync** (or the sync button) → log in with a `SYNC_USER` email +
password → your progress syncs to your server. Do a full sync/upload once from
the device that has your real collection so the server is seeded (subsequent
syncs are incremental).

---

## 3. How this relates to the AI service

| Concern | Where | Login |
|---|---|---|
| Collection + progress (cards, reviews, FSRS, concept tags) | self-hosted **sync server** | `SYNC_USER` account (this is the identity) |
| Grounded generation + tutor (AI) | **Supabase** Edge Functions | interim service token now; later exchanged from the sync session |

The AI service is currently reachable without per-user auth (`verify_jwt = false`)
— fine for a single-user/dev deployment. Before collecting any multi-user data
(e.g. the F3 calibration dataset), gate it: have the client exchange the sync
login for a short-lived service token, and enforce it in the Edge Functions.
That's the natural next identity step and doesn't change anything above.
