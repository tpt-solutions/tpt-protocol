# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- `cargo xtask new-protocol <name>`: scaffolds `crates/tpt-protocol-<name>` (manifest,
  source with a parser, validator and normalizer, tests, README, example), a fuzz target,
  and the wiring into the workspace, the `SourceProtocol` enum, the conformance list, the
  CDDL and JSON schema protocol lists, and the daemon.
- `--dry-run` prints the changes and writes nothing.
- `--root <dir>` operates on another checkout, which the tests use.
- Edits are computed in memory first. A missing anchor in an existing file is an error, and
  nothing is written.
- Protocol names are validated: lowercase letters, digits and single hyphens, starting with a
  letter.
- Replaces the earlier `scripts/new-protocol-crate.sh`, which is removed.
- Unit tests for name derivation and rejection, the anchored edits, and the dry run.
