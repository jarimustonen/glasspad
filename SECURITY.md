# Security Policy

glasspad is a loopback-only HTML-artifact host. Its security promise is that each
artifact renders in a **null-origin sandboxed iframe** and cannot escape that sandbox,
exfiltrate data, or reach the host filesystem outside its space. The threat surface worth
reporting against is therefore: the **local HTTP server** and its headers/CSP, the
**iframe sandbox and same-space bridge**, the **subprocess** used to open a browser, and
the **legacy data parser** (`glasspad data`, which reads untrusted CSV/JSON/mbox files).

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues, discussions,
or pull requests** — that discloses the issue before a fix is available.

Report privately using **GitHub's [Private Vulnerability Reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)**:
open the repository's **Security** tab → **Report a vulnerability**. If that is
unavailable, contact **jari@itsellesi.fi** privately.

Include, as far as you can:

- the affected version or commit;
- the component and threat surface — e.g. a sandbox escape from an artifact iframe, a
  cross-space data leak via the bridge, a path-traversal or symlink escape when serving a
  space, a crash/RCE in the `glasspad data` parser, or an argument-injection into the
  browser-open subprocess;
- reproduction steps or a proof-of-concept artifact/file;
- the impact you observed.

## What to Expect

- We will acknowledge your report as soon as we can and let you know whether we can
  reproduce it.
- We will confirm the issue, determine its severity, and keep you informed of progress.
- We practise **coordinated disclosure**: please give us a reasonable window to release a
  fix before any public disclosure. We will credit you for the finding unless you prefer
  to remain anonymous.

## Safe Harbor

We consider good-faith security research conducted under this policy to be authorized. We
will not pursue or support legal action against researchers who act in good faith, avoid
privacy violations and disruption to others, only interact with their own local instance,
and give us a reasonable time to respond before public disclosure.
