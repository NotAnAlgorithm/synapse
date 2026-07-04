# Copyright: Ankitects Pty Ltd and contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Client-side access to the Synapse AI service (Supabase Edge Functions).

Design constraint (notes/M2_service_layer_design.md §1-2): the Rust core makes
NO network calls; the Qt client shell reaches the hosted service over HTTPS and
commits results through the normal core RPCs (add_note, ...). The service is
OPTIONAL — when it is not configured, callers degrade cleanly to an "AI
unavailable" state and the base study loop is unaffected.

This module imports only ``anki.*`` + stdlib + ``requests`` (no ``aqt``/Qt), so
it stays headless-loadable and unit-testable. All calls are BLOCKING HTTPS — run
them off the UI thread via ``QueryOp``.

Configuration (generic collection config, ``synapse:`` namespaced):

* ``synapse:service_url``   — base URL of the Edge Functions, e.g.
  ``"https://<ref>.supabase.co/functions/v1"``. Empty ⇒ service disabled.
* ``synapse:service_key``   — the Supabase publishable/anon key, sent as the
  gateway ``apikey`` header.
* ``synapse:service_token`` — bearer token for the service (interim: the shared
  dev token; later: a per-user token from the identity layer). Falls back to
  ``service_key`` when unset.
"""

from __future__ import annotations

from typing import Any

import requests

import anki.collection

SERVICE_URL_KEY = "synapse:service_url"
SERVICE_KEY_KEY = "synapse:service_key"
SERVICE_TOKEN_KEY = "synapse:service_token"

# Generation can call an LLM, so allow a generous timeout; retrieval/tutor are
# quicker but share the ceiling for simplicity.
_TIMEOUT_SECS = 90


class ServiceError(Exception):
    """Any failure reaching or using the Synapse AI service."""


class ServiceNotConfigured(ServiceError):
    """The service URL is unset — the AI features are turned off."""


def _cfg(col: anki.collection.Collection, key: str) -> str:
    value = col.get_config(key, default="")
    return value if isinstance(value, str) else ""


def service_url(col: anki.collection.Collection) -> str:
    """The configured Edge-Functions base URL (trailing slash trimmed), or ""."""
    return _cfg(col, SERVICE_URL_KEY).rstrip("/")


def is_configured(col: anki.collection.Collection) -> bool:
    """True once a service URL is set; gate AI affordances on this."""
    return bool(service_url(col))


def _headers(col: anki.collection.Collection) -> dict[str, str]:
    key = _cfg(col, SERVICE_KEY_KEY)
    token = _cfg(col, SERVICE_TOKEN_KEY) or key
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if key:
        # Supabase's gateway wants the anon/publishable key as `apikey`.
        headers["apikey"] = key
    return headers


def _post(
    col: anki.collection.Collection, function: str, payload: dict[str, Any]
) -> Any:
    """POST JSON to one Edge Function and return the decoded body.

    Raises :class:`ServiceNotConfigured` when the service is off, or
    :class:`ServiceError` on transport / HTTP / decode failure — callers show a
    graceful "AI unavailable" message rather than crashing the study loop.
    """
    base = service_url(col)
    if not base:
        raise ServiceNotConfigured("The Synapse AI service is not configured.")
    try:
        resp = requests.post(
            f"{base}/{function}",
            json=payload,
            headers=_headers(col),
            timeout=_TIMEOUT_SECS,
        )
    except requests.RequestException as exc:
        raise ServiceError(f"could not reach the Synapse service: {exc}") from exc

    if resp.status_code >= 400:
        raise ServiceError(f"{function} failed ({resp.status_code}): {resp.text[:500]}")
    try:
        return resp.json()
    except ValueError as exc:
        raise ServiceError(f"{function} returned a non-JSON response") from exc


def retrieve(
    col: anki.collection.Collection,
    concept_tags: list[str],
    query: str,
    top_k: int = 5,
) -> list[dict[str, Any]]:
    """Concept-scoped retrieval; returns the grounding chunks (possibly empty)."""
    data = _post(
        col,
        "retrieve",
        {"concept_tags": concept_tags, "query": query, "top_k": top_k},
    )
    return data.get("chunks", []) if isinstance(data, dict) else []


def generate(
    col: anki.collection.Collection,
    concept_tag: str,
    instruction: str = "",
) -> dict[str, Any]:
    """Request a grounded DRAFT item for a concept (never auto-approved)."""
    result = _post(
        col, "generate", {"concept_tag": concept_tag, "instruction": instruction}
    )
    return result if isinstance(result, dict) else {}


def tutor_turn(
    col: anki.collection.Collection, payload: dict[str, Any]
) -> dict[str, Any]:
    """Send a student-state bundle to the tutor endpoint; return its turn(s)."""
    result = _post(col, "tutor", payload)
    return result if isinstance(result, dict) else {}
