#!/bin/sh
# Records a real Modbus TCP exchange on the loopback interface, for the daemon's pcap
# replay tests. A third-party server (pymodbus 3.6.9) answers a third-party client,
# and tcpdump records the TCP segments between them.
#
# Usage (Linux, root, python3 with venv support):
#   sudo sh scripts/capture-modbus-loopback.sh OUTPUT.pcap
#
# The checked-in fixture is crates/tpt-protocol-daemon/samples/captures/modbus-loopback.pcap.
set -e
OUT="${1:?usage: $0 OUTPUT.pcap}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

python3 -m venv "$WORK/venv"
"$WORK/venv/bin/pip" install --quiet pymodbus==3.6.9

cat > "$WORK/server.py" <<'PY'
import asyncio
from pymodbus.server import StartAsyncTcpServer
from pymodbus.datastore import ModbusSequentialDataBlock, ModbusSlaveContext, ModbusServerContext

async def main():
    # Holding registers 100-109 hold 1000..1009.
    block = ModbusSequentialDataBlock(0, [0] * 100 + [1000 + i for i in range(10)] + [0] * 50)
    ctx = ModbusServerContext(slaves=ModbusSlaveContext(hr=block, ir=block, co=block, di=block), single=True)
    await StartAsyncTcpServer(context=ctx, address=("127.0.0.1", 5020))

asyncio.run(main())
PY

cat > "$WORK/client.py" <<'PY'
from pymodbus.client import ModbusTcpClient

c = ModbusTcpClient("127.0.0.1", port=5020)
assert c.connect()
r = c.read_holding_registers(100, count=10, slave=1)
print("read", r.registers if not r.isError() else r)
r = c.read_holding_registers(4000, count=10, slave=1)  # out of range: exception response
print("exception", r.isError())
c.close()
PY

"$WORK/venv/bin/python" "$WORK/server.py" &
SERVER=$!
sleep 3
tcpdump -i lo -U -w "$OUT" 'tcp port 5020' &
TCPDUMP=$!
sleep 2
"$WORK/venv/bin/python" "$WORK/client.py"
sleep 1
kill -INT $TCPDUMP
sleep 1
kill $SERVER 2>/dev/null || true
echo "wrote $OUT"
