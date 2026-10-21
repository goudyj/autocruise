#!/usr/bin/env bash
# Persistent display for the user's own Chrome, readable through accessibility.
#
# Starts (if not already running), all as the current user, nothing installed:
#   1. an X display (Xvfb, or Xtigervnc with --vnc so you can look at it),
#   2. a private D-Bus session bus for it (accessibility is found through it),
#   3. your normal Google Chrome (default profile, so your logins are kept)
#      with --force-renderer-accessibility. No remote debugging port.
#
# Usage:
#   deploy/desktop/browser-desktop.sh start [--vnc] [URL]
#   deploy/desktop/browser-desktop.sh env      # prints the variables for autocruise
#   deploy/desktop/browser-desktop.sh status
#   deploy/desktop/browser-desktop.sh stop
#   deploy/desktop/browser-desktop.sh run [--vnc] [URL]    # start, then stay in the
#                                  foreground until Chrome or the display exits
#   deploy/desktop/browser-desktop.sh install-service [--vnc]   # systemd user unit
#   deploy/desktop/browser-desktop.sh uninstall-service
#
# Settings (environment): AUTOCRUISE_DISPLAY_NUM (default 50),
# AUTOCRUISE_VNC_PORT (default 5950, with --vnc), AUTOCRUISE_VNC_PASSWD
# (default ~/.config/autocruise/vncpasswd, create it with tigervncpasswd),
# CHROME (default google-chrome).
set -euo pipefail

num="${AUTOCRUISE_DISPLAY_NUM:-50}"
display=":${num}"
state="${XDG_RUNTIME_DIR:-/tmp}/autocruise-desktop-${num}"
bus="${state}/bus"
chrome="${CHROME:-google-chrome}"
vnc_port="${AUTOCRUISE_VNC_PORT:-5950}"
vnc_passwd="${AUTOCRUISE_VNC_PASSWD:-$HOME/.config/autocruise/vncpasswd}"

running() { [ -f "$state/$1.pid" ] && kill -0 "$(cat "$state/$1.pid")" 2>/dev/null; }

print_env() {
  echo "AUTOCRUISE_DESKTOP_DISPLAY=$display"
  echo "AUTOCRUISE_DESKTOP_DBUS=unix:path=$bus"
}

start() {
  local vnc=0 url=""
  for arg in "$@"; do
    case "$arg" in
      --vnc) vnc=1 ;;
      *) url="$arg" ;;
    esac
  done
  mkdir -p "$state"
  chmod 700 "$state"

  if ! running x; then
    if [ "$vnc" = 1 ]; then
      [ -f "$vnc_passwd" ] || { echo "missing $vnc_passwd: run 'tigervncpasswd $vnc_passwd' first" >&2; exit 1; }
      setsid -f Xtigervnc "$display" -geometry 1920x1080 -depth 24 -localhost yes \
        -rfbport "$vnc_port" -SecurityTypes VncAuth -PasswordFile "$vnc_passwd" \
        -AlwaysShared >"$state/x.log" 2>&1 </dev/null
    else
      setsid -f Xvfb "$display" -screen 0 1920x1080x24 -nolisten tcp >"$state/x.log" 2>&1 </dev/null
    fi
    for _ in $(seq 50); do [ -S "/tmp/.X11-unix/X${num}" ] && break; sleep 0.1; done
    pgrep -n -f "X(vfb|tigervnc) $display( |$)" >"$state/x.pid"
  fi

  if ! running dbus; then
    rm -f "$bus"
    setsid -f dbus-daemon --session --nofork --address="unix:path=$bus" \
      >"$state/dbus.log" 2>&1 </dev/null
    for _ in $(seq 50); do [ -S "$bus" ] && break; sleep 0.1; done
    pgrep -n -f "dbus-daemon --session --nofork --address=unix:path=$bus" >"$state/dbus.pid"
  fi

  if ! running chrome; then
    if pgrep -u "$(id -u)" -x chrome >/dev/null; then
      echo "Chrome is already running elsewhere (one instance per profile): close it first" >&2
      exit 1
    fi
    # The shell may carry a proxy meant for other tools: browse directly.
    env -u HTTP_PROXY -u HTTPS_PROXY -u NODE_USE_ENV_PROXY -u http_proxy -u https_proxy \
      DISPLAY="$display" DBUS_SESSION_BUS_ADDRESS="unix:path=$bus" \
      setsid -f "$chrome" --force-renderer-accessibility --no-first-run \
      --no-default-browser-check --window-position=0,0 --window-size=1920,1080 \
      ${url:+"$url"} >"$state/chrome.log" 2>&1 </dev/null
    sleep 2
    pgrep -o -u "$(id -u)" -x chrome >"$state/chrome.pid" || true
  fi
  status
  echo
  print_env
}

status() {
  for p in x dbus chrome; do
    if running "$p"; then echo "$p: running (pid $(cat "$state/$p.pid"))"; else echo "$p: stopped"; fi
  done
}

stop() {
  for p in chrome dbus x; do
    if running "$p"; then kill "$(cat "$state/$p.pid")" || true; fi
    rm -f "$state/$p.pid"
  done
}

# Foreground mode for a service manager: start everything, then wait until
# Chrome or the display goes away, stop the rest and fail so the manager
# restarts it.
run() {
  start "$@"
  while running x && running dbus && running chrome; do sleep 10; done
  echo "Chrome or the display exited; stopping" >&2
  stop
  exit 1
}

unit_name="autocruise-desktop.service"
unit_path="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/$unit_name"

install_service() {
  local args="" script
  [ "${1:-}" = "--vnc" ] && args=" --vnc"
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  mkdir -p "$(dirname "$unit_path")"
  cat >"$unit_path" <<EOF
# Installed by $script install-service
[Unit]
Description=autocruise: persistent display with the user's Chrome
After=default.target

[Service]
Type=simple
Environment=AUTOCRUISE_DISPLAY_NUM=$num
ExecStart=$script run$args https://www.sncf-connect.com/en-en
ExecStop=$script stop
Restart=on-failure
RestartSec=15

[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload
  if pgrep -u "$(id -u)" -x chrome >/dev/null && ! running chrome; then
    echo "Chrome is running outside this script: close it, then run" >&2
    echo "  systemctl --user enable --now $unit_name" >&2
    systemctl --user enable "$unit_name"
    return
  fi
  # A display started by hand would make the unit exit at once: hand it over.
  stop
  systemctl --user enable --now "$unit_name"
  systemctl --user --no-pager status "$unit_name" | head -5
  if [ "$(loginctl show-user "$(id -un)" -p Linger --value 2>/dev/null)" != yes ]; then
    echo "note: without 'loginctl enable-linger $(id -un)' the service stops when you log out" >&2
  fi
}

uninstall_service() {
  systemctl --user disable --now "$unit_name" 2>/dev/null || true
  rm -f "$unit_path"
  systemctl --user daemon-reload
}

case "${1:-}" in
  start) shift; start "$@" ;;
  run) shift; run "$@" ;;
  install-service) shift; install_service "$@" ;;
  uninstall-service) uninstall_service ;;
  env) print_env ;;
  status) status ;;
  stop) stop ;;
  *) sed -n '2,26p' "$0"; exit 2 ;;
esac
