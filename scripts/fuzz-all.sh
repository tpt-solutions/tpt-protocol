#!/bin/sh
# Runs every cargo-fuzz target for a fixed time each, and records the exit status.
# Linux only (libFuzzer does not link on Windows MSVC). Needs nightly and cargo-fuzz.
#
# Usage, from the repository root on Linux:
#   sh scripts/fuzz-all.sh [SECONDS_PER_TARGET] [LOG_DIR]
#
# A crash leaves its input in fuzz/artifacts/<target>/; the summary line says so.
set -u
SECONDS_PER_TARGET="${1:-120}"
LOG_DIR="${2:-/root/fuzz-logs}"
mkdir -p "$LOG_DIR"
SUMMARY="$LOG_DIR/summary.txt"
: > "$SUMMARY"

TARGETS="bacnet canopen coap dnp3 doip-someip ethercat hl7v2 ieee11073-sdc isobus j1939 knx matter-thread mavlink modbus nmea ocpp opcua sparkplugb tup_cbor tup_json uds"
for target in $TARGETS; do
  cargo +nightly fuzz run "$target" --fuzz-dir fuzz -- -max_total_time="$SECONDS_PER_TARGET" \
    > "$LOG_DIR/$target.log" 2>&1
  status=$?
  runs=$(grep -o 'stat::number_of_executed_units: [0-9]*' "$LOG_DIR/$target.log" | tail -1 | awk '{print $2}')
  crashes=$(ls "fuzz/artifacts/$target" 2>/dev/null | wc -l)
  echo "$target exit=$status runs=${runs:-?} artifacts=$crashes" >> "$SUMMARY"
done
echo DONE >> "$SUMMARY"
