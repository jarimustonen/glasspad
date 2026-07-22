# Design — Security & origin isolation model

This is the part that must be right. Letting an agent supply arbitrary HTML +
JS is the whole point of the rewrite, and it directly conflicts with today's
allowlist sanitizer. This document defines how arbitrary agent content is
rendered safely across the three deployment modes.

## 1. Threat model

- **Content author = the agent.** Semi-trusted: it acts for the user, but its
  output can be prompt-injected into hostile HTML/JS.
- **Viewer = the user's browser**, same browser that talks to the Glasspad app
  and API.
- **Deployment tiers**: localhost (single user), team server (multiple users,
  one host), glasspad.ai (multi-tenant cloud).

Concrete risks if agent JS runs same-origin with the app:

1. It calls the Glasspad API (`DELETE /api/spaces/...`, read other spaces).
2. It reads app cookies / localStorage / tokens.
3. On a shared host, it attacks *other users'* spaces (cross-tenant).
4. It exfiltrates page/user data to an external server.

## 2. Defense 1 — sandboxed iframe (all modes)

Every artifact is rendered inside:

```html
<iframe sandbox="allow-scripts" src="…" ></iframe>
```

`allow-scripts` **without** `allow-same-origin` gives the artifact a **null
origin**. It can run JS (charts, interactivity) but cannot:

- reach the parent frame (the trusted nav chrome),
- read app cookies / localStorage,
- make same-origin requests to the Glasspad API.

The **navigation chrome lives in the parent document** (Glasspad-authored,
trusted). Untrusted content lives only in the iframe. This separation is what
makes arbitrary agent HTML safe. It is the model Claude's own Artifacts use.

## 3. Defense 2 — separate content origin (team / cloud)

Sandbox alone is not enough on a shared host: browser bugs and future
`allow-same-origin` needs mean we should not co-locate untrusted content with
the app origin. So:

- **App / API** on the app origin (e.g. `glasspad.ai`).
- **Rendered artifact content** on a **distinct content origin**, ideally a
  **per-space subdomain** (`{space}.usercontent.glasspad.ai`).

Per-space subdomains give origin isolation **between spaces** too, so even a
sandbox escape in space A cannot reach space B (different origin). This mirrors
Claude's `claudeusercontent.com`.

**Degradation**: localhost cannot easily host two origins, so it runs
sandbox-iframe only (acceptable — no other users, no cross-tenant surface).

Open question for the PO: start with a single content origin and add per-space
subdomains later, or target wildcard-DNS + TLS per-space from the start? (Adds
deployment complexity: wildcard cert, DNS.)

## 4. Defense 3 — egress-restricting CSP on the artifact frame

Even sandboxed and cross-origin, artifact JS could `fetch()` an attacker server
to exfiltrate. The artifact response carries a CSP that:

- allows `script-src`/`style-src` inline (needed — agent writes inline JS/CSS),
- allows `connect-src`/`img-src`/`font-src` only `self` (the content origin) so
  the `/_gp/*` base libraries load,
- blocks arbitrary external hosts (no exfiltration, no external tracking),
- `frame-ancestors` limited to the app origin (only the Glasspad shell may frame
  the content).

This is a policy shift from today's CSP (which allows `cdn.jsdelivr.net`): base
libraries move **local** (`/_gp/*`) precisely so we can keep egress closed while
still shipping charts. Interactive AND locked down.

Trade-off to decide: strict `connect-src 'self'` blocks artifacts that
legitimately need to call an external API. Options: (a) forbid it (safest),
(b) per-space allowlist in `glasspad.yaml`, (c) relax only on localhost.

## 5. The parent ↔ iframe bridge

- `bridge.js` (auto-injected into fragment-wrapped artifacts) runs *inside* the
  iframe. It:
  - intercepts clicks on `a[href^="glasspad:"]` and `postMessage`s the parent to
    navigate/swap the iframe,
  - receives the current theme from the parent and applies it (theme sync).
- The parent validates every `postMessage` `origin` and only accepts a fixed
  message schema (navigate-to-slug). No `eval`, no arbitrary DOM injection from
  child → parent.
- Full-document artifacts opt out of the bridge; they cross-link with
  `target="_top"` and a real `/{space}/{slug}` path.

## 6. Why not keep the sanitizer?

Sanitizing to an allowlist (today's `sanitize.rs`) cannot express interactive
UIs — no `<script>`, no `<style>`, no event handlers. Extending the allowlist to
allow scripts defeats sanitization entirely. The correct boundary is the
**origin/sandbox**, not tag filtering. Sanitization may survive as an *optional*
"static/safe" render mode for callers who want it, but it is no longer the
primary mechanism.

## 7. Summary of layered defenses

| Layer | localhost | team | cloud |
|---|---|---|---|
| Sandbox iframe (null origin) | ✅ | ✅ | ✅ |
| Separate content origin | — | ✅ | ✅ |
| Per-space subdomain isolation | — | optional | ✅ |
| Egress-restricting CSP | ✅ | ✅ | ✅ |
| Validated postMessage bridge | ✅ | ✅ | ✅ |
