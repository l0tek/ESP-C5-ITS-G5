#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/firmware"
port="${1:-}"
if [ -z "$port" ]; then
  shopt -s nullglob
  ports=(/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_*)
  if [ "${#ports[@]}" -ne 1 ]; then
    echo "Bitte genau einen ESP anschließen oder den USB-Port als Argument angeben." >&2
    exit 1
  fi
  port="${ports[0]}"
fi
cargo espflash flash --release --locked --chip esp32c5 --flash-size 4mb \
  --port "$port" --non-interactive --skip-update-check
