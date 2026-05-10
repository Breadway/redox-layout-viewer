#!/usr/bin/env bash
set -euo pipefail

user_name="${USER:-breadway}"

mapfile -t devices < <(
  find /dev/input/by-id -maxdepth 1 -type l \
    \( -name 'usb-Falbatech_Redox_vial:*event-kbd' -o -name 'usb-Falbatech_Redox_vial:*event-if02' \) \
    -print 2>/dev/null | sort
)

if [[ ${#devices[@]} -eq 0 ]]; then
  echo "No Redox keyboard event devices found under /dev/input/by-id." >&2
  exit 1
fi

echo "Granting ${user_name} rw access to Redox keyboard event nodes:"
for link in "${devices[@]}"; do
  target="$(readlink -f "$link")"
  echo "  $link -> $target"
  sudo setfacl -m "u:${user_name}:rw" "$target"
done

echo
echo "Verifying ACLs:"
for link in "${devices[@]}"; do
  target="$(readlink -f "$link")"
  getfacl -p "$target" | sed -n '1,8p'
  echo "---"
done

echo "Done. Relaunch the app and test typing." 
