#!/bin/sh
# Records an OPC UA Hello/OpenSecureChannel exchange between asyncua's client and asyncua's
# server on loopback, so the wire format of a reference server can be read off the capture.
# Needs Linux, root (for tcpdump), and python3 with venv support.
#
# Usage: sudo sh scripts/opcua-reference-capture.sh OUTPUT.pcap
set -e
OUT="${1:?usage: $0 OUTPUT.pcap}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
python3 -m venv "$WORK/venv"
"$WORK/venv/bin/pip" install --quiet asyncua==2.1.0

cat > "$WORK/server.py" <<'PY'
import asyncio
from asyncua import Server

async def main():
    server = Server()
    await server.init()
    server.set_endpoint("opc.tcp://127.0.0.1:4850/")
    async with server:
        await asyncio.sleep(12)

asyncio.run(main())
PY

cat > "$WORK/client.py" <<'PY'
import asyncio
from asyncua import Client

async def main():
    async with Client(url="opc.tcp://127.0.0.1:4850/") as client:
        print("connected")

asyncio.run(main())
PY

"$WORK/venv/bin/python" "$WORK/server.py" &
SERVER=$!
sleep 4
tcpdump -i lo -U -w "$OUT" 'tcp port 4850' &
TCPDUMP=$!
sleep 1
"$WORK/venv/bin/python" "$WORK/client.py"
sleep 1
kill -INT $TCPDUMP || true
kill $SERVER 2>/dev/null || true
echo "wrote $OUT"
