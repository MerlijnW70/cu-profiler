# Security Policy

## Supported versions

`cu-profiler` is pre-1.0 and under active development. Security fixes are applied
to the latest `main`. There is no long-term support branch yet.

## Reporting a vulnerability

Please **do not** open a public issue for security-sensitive reports.

Instead, use GitHub's private vulnerability reporting:
**Security → Report a vulnerability** on the repository, or open a private
security advisory.

Include, where possible:

- a description of the issue and its impact,
- steps or a minimal input (e.g. a recorded log) that reproduces it,
- the affected crate and version/commit.

We aim to acknowledge reports within a few business days.

## Scope notes

`cu-profiler` parses untrusted log text. The parser is designed to be tolerant —
malformed input should produce warnings and lowered confidence, never a panic or
unbounded resource use. Inputs that cause a panic, hang, or excessive memory use
in library code are considered security-relevant and are in scope.
