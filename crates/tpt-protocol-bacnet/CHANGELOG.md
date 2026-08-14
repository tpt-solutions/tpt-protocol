# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: BACnet/IP (building automation) support in the TPT Protocol suite.
- BVLC parsing in `BacnetParser`, requiring BVLC type `0x81` and decoding the BVLC
  function (UnicastNPDU `0x04`, BroadcastNPDU `0x05`, AddressResolutionRequest
  `0x07`, AddressResolutionResponse `0x08`, VirtualLayerResolution `0x0A`,
  ForwardedNPDU `0x09`) and the 16-bit big-endian BVLC length.
- APDU type decoding from the high nibble of the first APDU byte: ConfirmedRequest
  (0), UnconfirmedRequest (1), SimpleAck (2), ComplexAck (3), Error (5), Reject (6),
  Abort (7), with an `Unknown(u8)` fallback.
- Object-identifier (`object_type` 2 bytes + `instance` 4 bytes) and optional
  2-byte property-ID extraction from the APDU (`BacnetObjectId`).
- `BacnetState` / `BacnetValidator` (`StateValidator`) enforcing connection, a
  device-instance range (≤ 4,194,303), and a consecutive-error threshold (≤ 10).
- `BacnetNormalizer` (`DataNormalizer`) mapping frames to `tup::DataPoint`s keyed by
  `device_id = "bacnet:bvlc:{function}"` and `point_id = "obj:{type}:{instance}"`
  (or `"apdu:{apdu_type}"`), carrying the raw APDU payload or `0`.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
