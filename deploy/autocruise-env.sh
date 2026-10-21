#!/usr/bin/env bash
# Launches autocruise with credentials from a private env file, for MCP
# clients that cannot pass secrets to a server (e.g. OpenClaw 2026.9, whose
# mcp.servers.*.env only takes plain strings).
#
#   File: ${AUTOCRUISE_ENV_FILE:-~/.config/autocruise/env}, mode 600, lines KEY=value
#         (e.g. SNCF_API_TOKEN=...). Never inside the repository.
#   Binary: ${AUTOCRUISE_BIN:-~/.cargo/bin/autocruise}
#
# Use it as the MCP server command, with the usual arguments:
#   deploy/autocruise-env.sh mcp --enable sncf,sncf-connect
#
# Store or replace one value (typed at a hidden prompt, never echoed):
#   deploy/autocruise-env.sh --set SNCF_API_TOKEN
set -euo pipefail

env_file="${AUTOCRUISE_ENV_FILE:-$HOME/.config/autocruise/env}"
bin="${AUTOCRUISE_BIN:-$HOME/.cargo/bin/autocruise}"

if [ "${1:-}" = "--set" ]; then
  name="${2:?usage: autocruise-env.sh --set NAME}"
  case "$name" in *[!A-Z0-9_]*|[0-9]*) echo "invalid name: $name" >&2; exit 2 ;; esac
  read -rsp "$name: " value; echo
  [ -n "$value" ] || { echo "empty value, nothing changed" >&2; exit 1; }
  umask 077
  mkdir -p "$(dirname "$env_file")"
  tmp="$(mktemp "$env_file.XXXXXX")"
  { [ -f "$env_file" ] && grep -v "^$name=" "$env_file" || true; } >"$tmp"
  printf '%s=%s\n' "$name" "$value" >>"$tmp"
  mv "$tmp" "$env_file"
  echo "saved $name in $env_file (${#value} characters)"
  exit 0
fi

if [ -f "$env_file" ]; then
  mode="$(stat -c '%a' "$env_file")"
  if [ "${mode: -2}" != "00" ]; then
    echo "autocruise-env: $env_file is readable by others (mode $mode); run: chmod 600 $env_file" >&2
    exit 1
  fi
  set -a
  # shellcheck disable=SC1090
  . "$env_file"
  set +a
fi

exec "$bin" "$@"
