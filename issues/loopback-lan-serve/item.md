---
created: 2026-08-14
updated: 2026-08-14
type: feature
status: done
priority: high
commits:
- hash: 86a26e5
  summary: LAN-reachable loopback serve with DNS-rebinding guard kept
- hash: cf84ed8
  summary: harden LAN serve per 4-model security review
closed: 2026-08-14
---

# LAN-reachable loopback serve (share the local server to other devices, keep DNS-rebinding guard)

## Description

## Motivation (Jari 2026-08-13)

Jari works from a **different machine on the same LAN** than the one running `glasspad`. Today the loopback `serve` binds `127.0.0.1`, so the served space is only viewable on the host machine — **unusable** for Jari's actual setup. He wants the local serve to be **shared on the LAN**: reachable from other devices on the same local network. (Chosen path "L" over "push everything to hosted".)

## The constraint this fights — do NOT just bind 0.0.0.0

Loopback `serve` is **deliberately loopback-only** as a security property: a Host-header guard refuses any non-loopback `Host`, which is the **DNS-rebinding protection** (stops a malicious website from using the victim's browser to reach the local artifact host). Naively binding `0.0.0.0` + accepting any Host would drop that guard and expose the artifact host to DNS-rebinding + the whole LAN. This is why it is **design-first + security-sensitive**, not a one-line flag.

## Scope — design-first, then implement
Design and implement a **LAN-reachable serve mode** that keeps the security posture intact:

1. **Explicit opt-in, explicit address.** A flag (e.g. `glasspad loopback serve --bind <LAN-IP-or-host>` / `--lan`), NOT on by default. Prefer binding an **explicit** LAN address the user names over a blanket `0.0.0.0`.
2. **Keep DNS-rebinding protection.** Do not simply disable the Host guard. Extend it to an **allowlist**: accept loopback Hosts PLUS the explicitly-configured bind host/IP (and reject everything else). A rebinding attacker using a foreign Host still fails the guard.
3. **Preserve the rest of the trust model.** The trusted-shell/submit anti-spoof + CSRF checks (loopback Origin, shell token, size/rate caps) must keep working; adjust only what the LAN Host legitimately requires. Each artifact page stays a null-origin sandboxed iframe — the sandbox/CSP is untouched.
4. **Loud about the tradeoff.** Emit a clear warning at startup that the server is now reachable on the LAN (surface the exact URL/host), and document that this is a trusted-LAN convenience, not a public exposure. It carries no API key, so it must never be bound to a public interface.
5. Config: allow the bind address via `.glasspad.yaml` / home config per the existing per-key merge (a `bind`/`lan` key), consistent with the publish-first config model.

## Done criteria
- `glasspad loopback serve` can be opted into a LAN-reachable bind; another LAN device can load the space and its artifacts.
- The Host guard still rejects non-allowlisted Hosts (DNS-rebinding stays blocked); add adversarial probes to `./test-security.sh` (a foreign-Host request to the LAN-bound server is refused; the sandbox/CSP/airlock are unchanged).
- Default behaviour (no flag) is byte-compatible loopback-only.
- Startup warning + docs (`src/skill.md` if it advertises serve) explain the posture.
- `/llm-review` (+ `/assess-findings`) before merge — this loosens a security guard, so review is mandatory. `./test-security.sh` (48 + Wave 2a + new LAN probes) green.

## Related / lands
- Lane B (server/CLI/loopback core): `src/cli.rs`, `src/server.rs`, the loopback Host guard, config (`src/config.rs`).
- Historically `tw view` bridged server→seat for exactly this reason; this makes the local serve natively LAN-reachable instead.

## Design decisions (implementation, 2026-08-14)

- **Flag:** `glasspad loopback serve --bind <LAN-IP-or-host>` (OFF by default).
  Resolution precedence (AI-first §8): `--bind` flag > `$GLASSPAD_BIND` > config
  `bind:` key (`.glasspad.yaml` → home). No flag → byte-compatible loopback-only.
- **Bind model (two listeners, not `0.0.0.0`):** loopback `127.0.0.1:port` is
  ALWAYS bound (so `await-submission`/`open`/`stop`, which all speak loopback HTTP,
  keep working). `--bind <HOST>` ADDITIONALLY binds the resolved non-loopback
  address(es) of `<HOST>`. Wildcard (`0.0.0.0`/`::`) and loopback values are
  rejected — the issue's "explicit address over a blanket 0.0.0.0". A hostname is
  resolved to its non-loopback addrs to bind; the literal host string is what the
  allowlist matches.
- **DNS-rebinding guard = allowlist, still fail-closed.** The `host_guard` state
  becomes `HostPolicy{port, allow_host: Option<String>}`. Accepted Hosts: the two
  loopback names PLUS (LAN mode) the one configured `<HOST>` (case-insensitive
  host, exact port). Every other Host — a rebinding attacker name, a foreign IP —
  is still `421`-refused.
- **What the LAN Host legitimately requires:** the artifact CSP and the submit
  CSRF `Origin` allowlist gain the LAN origin `http://<host>:<port>` (carried on
  `OriginPolicy::Loopback{ lan }`), so a LAN client's shell + `/_gp/v1/*` base
  libs load and its trusted-shell submit is accepted. The sandbox/`connect-src
  'none'`/Trusted-Types/`allow-forms`-absent boundary is UNCHANGED — the artifact
  stays a null-origin sandboxed iframe.
- **Loud startup warning** naming the exact reachable URL; documents it as a
  trusted-LAN convenience carrying no API key, never a public bind.

## Review-driven hardening (post `/llm-review`, 4-model panel)

The security panel (gemini/openai/anthropic/deepseek) reached 4/4 consensus that
the first cut was too permissive. Applied:

- **`--bind` accepts a literal private IPv4 ONLY.** Hostnames are refused — a
  hostname in the Host allowlist reintroduces DNS rebinding (the browser keeps
  sending the name while DNS is repointed), and a hostname resolving to `0.0.0.0`
  bypassed the wildcard check. A literal IP cannot be rebound. This also removes the
  multi-A-record / resolve-TOCTOU / IPv6-via-DNS surface.
- **Private-range enforced:** only RFC1918 / link-local (169.254) / CGNAT (100.64/10)
  bind; wildcard, loopback, IPv6, and public/globally-routable IPs are hard errors —
  so "never a public bind" is *enforced*, not merely asserted.
- **`bind:` is HOME-config-only.** A repo-local `.glasspad.yaml bind:` is ignored
  (and warned about) so a cloned repo cannot silently opt a machine into a LAN bind.
- **Guard defense-in-depth:** the Host guard also rejects a foreign HTTP/1.1
  absolute-form / HTTP-2 `:authority` (must independently pass the allowlist) and a
  duplicate `Host` header.
- **Loud warning printed BEFORE the serving envelope**, names plaintext-HTTP /
  MITM risk; default HTTP port (80) omitted from the LAN origin so CSRF matches.
- **`serve_on_all` aborts sibling listeners** explicitly on early return.

**Accepted residual (trusted-LAN threat model, documented, not a code fix):** over
plaintext HTTP a LAN MITM can read submissions/content and inject same-origin HTML
at the LAN origin (which the artifact CSP now names). The artifact stays null-origin
sandboxed with `connect-src 'none'`; this is the inherent cost of a plaintext-LAN
convenience and is called out in the startup warning. HTTPS for the LAN bind is out
of scope for this feature.
