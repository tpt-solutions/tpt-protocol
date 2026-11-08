# Security Policy

## Supported versions

Only the latest published release of each `tpt-protocol-*` crate and the
`main` branch receive security fixes.

## Reporting a vulnerability

Please **do not** open a public issue for a security problem.

Report it privately through GitHub's "Report a vulnerability" button on the
repository's Security tab (a private security advisory). Include:

- the crate and version (or commit) affected,
- a minimal input (frame bytes or a test case) that triggers the problem,
- the observed impact (panic, out-of-bounds read, unbounded allocation, auth bypass, …).

You can expect an acknowledgement within 7 days. Once a fix is ready we will
coordinate a release and credit you in the changelog unless you ask otherwise.

## Scope notes

- The protocol crates parse untrusted bytes. Any panic, integer overflow, or
  unbounded allocation reachable from `FrameParser::parse`, `try_parse`, or a
  `DataNormalizer` is in scope.
- The daemon's WebSocket API binds to `127.0.0.1` by default. Host, Origin,
  and optional token checks apply on a loopback bind. Exposing it on a
  non-loopback address without `--token` is a deployment choice, not a
  vulnerability.
