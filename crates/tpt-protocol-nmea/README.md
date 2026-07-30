# tpt-protocol-nmea

NMEA 0183 marine protocol parser for the TPT telemetry pipeline.

## Implementation

- **Format**: `$<talker><sentence>,<fields>*<checksum_hex>\r\n` with XOR checksum
- **Talkers**: GP (GPS), SD (depth), GI, HE, IN
- **Sentences**: GGA (GPS fix data), RMC (recommended minimum), DBT (depth),
  VHW (water speed), MWV (wind)
- **Parser**: Validates UTF-8, `$` prefix, `*` separator, checksum hex format;
  computes XOR checksum and reports validity without rejecting on mismatch
- **Validator**: Checks connection, signal quality range (0-5), error thresholds
- **Normalizer**: Maps to DataPoints with raw sentence attached
