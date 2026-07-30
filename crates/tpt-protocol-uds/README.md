# tpt-protocol-uds

ISO 14229 (UDS) automotive diagnostic protocol parser for the TPT telemetry pipeline.

## Implementation

- **SIDs**: DiagnosticSessionControl (0x10), ECUReset (0x11), ClearDiagnosticInfo (0x14),
  SecurityAccess (0x27), CommunicationControl (0x28), TestPresenter (0x3E),
  ReadDataByIdentifier (0x22), WriteDataByIdentifier (0x2E), ReadMemoryByAddress (0x23)
- **Response**: SID | 0x40 for response frames
- **Sub-functions**: Parsed for SIDs 0x10/0x11/0x14/0x27/0x28/0x3E (and their responses)
- **Parser**: Validates non-empty input; handles request vs. response detection
- **Validator**: Checks connection, session type validity, error thresholds
- **Normalizer**: Maps to DataPoints with SID-based point IDs
