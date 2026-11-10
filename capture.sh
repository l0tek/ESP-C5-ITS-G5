#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
rustc --edition=2021 -O host/src/main.rs -o cits-pcap
input="${1:-}"
if [ -z "$input" ]; then
  shopt -s nullglob
  ports=(/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_*)
  if [ "${#ports[@]}" -ne 1 ]; then
    echo "Bitte genau einen ESP anschließen oder den Eingabepfad als Argument angeben." >&2
    exit 1
  fi
  input="${ports[0]}"
fi
if [ -c "$input" ]; then
  saved_tty="$(stty -F "$input" -g)"
  trap 'stty -F "$input" "$saved_tty" 2>/dev/null || true' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  stty -F "$input" raw -echo
fi
./cits-pcap "$input" "${2:-capture.pcap}"
