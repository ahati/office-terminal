# Security Policy

## Supported Versions

Office Terminal is pre-1.0. Security fixes are applied to the latest `0.1.x`
release and to `master`.

| Version | Supported |
| :--- | :--- |
| 0.1.x | ✅ |
| < 0.1 | ❌ |

## Reporting a Vulnerability

Please do **not** open a public issue for an undisclosed vulnerability.

Preferred channel: use GitHub's private vulnerability reporting on the
repository's **Security** tab.

If private reporting is unavailable, email **amitesh.hati@gmail.com** with:

- the affected version or commit,
- a minimal reproducer or document that triggers the issue,
- the expected and observed behavior,
- any relevant logs or backtraces.

You should receive an acknowledgement within a few business days. We will
coordinate a disclosure timeline with you and credit reporters in the release
notes unless you prefer to remain anonymous.

## Scope

Office Terminal parses untrusted `.pptx`, `.pdf`, and `.docx` packages. Reports
about parser crashes, panics, out-of-bounds reads/writes, infinite loops,
resource-exhaustion (zip bombs, deeply nested XML, oversized media), path
traversal, or terminal-escape injection are in scope. Reports about the
rendering fidelity of a particular document are ordinary bugs, not security
issues, unless they also trigger one of the above.

## Safe Harbor

We will not pursue legal action against researchers who make a good-faith
effort to avoid privacy violations, data destruction, and service disruption
while investigating a vulnerability, and who report the issue privately before
public disclosure.
