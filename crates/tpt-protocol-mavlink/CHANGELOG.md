# Changelog

All notable changes to this crate are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate uses [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-01

### Added
- Initial release: MAVLink protocol crate that wraps the mavlink crate for frame decoding.

### Changed
- Renamed from `out-protocol-mavlink`. Decoding now uses the `mavlink` 0.15 crate instead of a hand-written parser. Publishing is enabled.
- `MavlinkFrame` no longer exposes the wire checksum.
