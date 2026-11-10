#!/usr/bin/env python3
"""Endpoint test: the daemon's MAVLink decoding against frames made by pymavlink.

pymavlink, the reference Python MAVLink library, encodes HEARTBEAT, SYS_STATUS and
GPS_RAW_INT messages as MAVLink 2 frames. They are sent as UDP datagrams to the daemon
(`--simulate mavlink --udp-bind`), which decodes them and records each field as a
point. The test checks every recorded field against the value that was encoded.

Usage (from the repository root):

    cargo build -p tpt-protocol-daemon
    pip install pymavlink==2.4.50
    python scripts/endpoints/mavlink_pymavlink.py target/debug/tpt-protocol-daemon.exe
"""

import io
import sys

from common import Daemon, report, send_datagram

PORT = 14561
WS_PORT = 19081
DEVICE = "mavlink:1:1"


def main(binary):
    from pymavlink.dialects.v20 import common as mav

    mav.WIRE_PROTOCOL_VERSION = "2.0"
    encoder = mav.MAVLink(io.BytesIO(), srcSystem=1, srcComponent=1)

    heartbeat = mav.MAVLink_heartbeat_message(
        type=2,  # quadrotor
        autopilot=3,  # ArduPilot
        base_mode=81,
        custom_mode=1234,
        system_status=4,  # active
        mavlink_version=3,
    )
    sys_status = mav.MAVLink_sys_status_message(
        onboard_control_sensors_present=0,
        onboard_control_sensors_enabled=0,
        onboard_control_sensors_health=0,
        load=512,  # 51.2 %
        voltage_battery=11_850,  # mV
        current_battery=-120,  # cA
        battery_remaining=76,  # percent
        drop_rate_comm=0,
        errors_comm=0,
        errors_count1=0,
        errors_count2=0,
        errors_count3=0,
        errors_count4=0,
    )
    gps = mav.MAVLink_gps_raw_int_message(
        time_usec=0,
        fix_type=3,  # 3D fix
        lat=-331_234_567,  # -33.1234567 degrees
        lon=1_512_345_678,  # 151.2345678 degrees
        alt=54_321,  # 54.321 m
        eph=100,
        epv=200,
        vel=0,
        cog=0,
        satellites_visible=9,
    )

    expected = [
        (DEVICE, "HEARTBEAT.custom_mode", 1234, None),
        (DEVICE, "HEARTBEAT.type", 2, None),
        (DEVICE, "HEARTBEAT.autopilot", 3, None),
        (DEVICE, "HEARTBEAT.base_mode", 81, None),
        (DEVICE, "HEARTBEAT.system_status", 4, None),
        (DEVICE, "HEARTBEAT.mavlink_version", 3, None),
        (DEVICE, "SYS_STATUS.voltage_battery", 11_850, "mV"),
        (DEVICE, "SYS_STATUS.current_battery", -120, "cA"),
        (DEVICE, "SYS_STATUS.battery_remaining", 76, "%"),
        (DEVICE, "SYS_STATUS.load", 512, "0.1%"),
        (DEVICE, "GPS_RAW_INT.lat", -33.1234567, "deg"),
        (DEVICE, "GPS_RAW_INT.lon", 151.2345678, "deg"),
        (DEVICE, "GPS_RAW_INT.alt", 54.321, "m"),
        (DEVICE, "GPS_RAW_INT.satellites_visible", 9, None),
        (DEVICE, "GPS_RAW_INT.fix_type", 3, None),
    ]

    daemon = Daemon(binary, "mavlink", PORT, WS_PORT)
    try:
        for message in (heartbeat, sys_status, gps):
            send_datagram(PORT, message.pack(encoder))
        recorded = daemon.points()
    finally:
        daemon.stop()
    return report("mavlink (pymavlink 2.4.50)", expected, recorded)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
