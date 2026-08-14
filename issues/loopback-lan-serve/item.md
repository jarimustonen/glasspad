---
created: 2026-08-14
updated: 2026-08-14
type: feature
status: open
priority: high
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
