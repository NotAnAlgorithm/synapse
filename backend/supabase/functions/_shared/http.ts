// Small shared HTTP helpers for the Edge Functions (JSON responses + CORS).
// No secrets, no external deps.

export const jsonHeaders = {
  "Content-Type": "application/json",
  // Permissive CORS is fine for a scaffold; tighten to the client origin(s)
  // once the deployment domain is known.
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Access-Control-Allow-Headers": "authorization, content-type",
};

export function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: jsonHeaders });
}

/** Handle a CORS preflight; returns a Response for OPTIONS, else null. */
export function handlePreflight(req: Request): Response | null {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: jsonHeaders });
  }
  return null;
}
