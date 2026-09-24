#!/usr/bin/env bash
# Screenshots the standalone panel of one revision. The second argument is the
# DPI scale and the third the revision, so
#   ./shot.sh doc/panel.png 1.5 d
# grabs the Rev D panel at one and a half times its own coordinates -- 1680 x
# 441 rather than the 1120 x 294 the window opens at. The panel is drawn rather
# than pictured, so the lettering is sharp at any scale, but the pictures in it
# are not: the knob renders are 208 and 176 pixel frames drawn about 100 and 80
# across, and the switch caps are 119 to 127 pixels drawn about 50 tall, so they
# hold up to about two; the meter photograph to about three. Targets the window
# by address and refuses to act unless the focus actually landed on it -- a
# title selector that finds nothing falls back to whatever is focused, which is
# how a terminal ended up floated across the screen.
set -euo pipefail
out=${1:?usage: shot.sh <output.png> [dpi-scale] [a|d|f]}
dpi=${2:-1}
rev=${3:-d}
case "$rev" in
    a | d | f) ;;
    *)
        echo "shot.sh: the revision is a, d or f, not '$rev'" >&2
        exit 2
        ;;
esac
bin="comp76fx_rev_$rev"
title="Comp76Fx Rev ${rev^^}"
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# Built every time, which is nothing when it is current. The standalone binary
# is a separate target from the plugin, so bundling does not rebuild it, and a
# binary left from an earlier build would show an earlier panel.
cargo build --release --quiet --manifest-path "$project_dir/Cargo.toml" \
    -p "$bin" --features standalone

pkill -x "$bin" 2>/dev/null || true
sleep 0.5
XDG_CONFIG_HOME="${SHOT_CONFIG:-$HOME/.config}" "$project_dir/target/release/$bin" --backend dummy --dpi-scale "$dpi" >/dev/null 2>&1 &
sleep 3

addr=$(hyprctl clients -j | python3 -c "
import json,sys
for c in json.load(sys.stdin):
    if c['title'] == sys.argv[1]:
        print(c['address']); break
" "$title")
[ -n "$addr" ] || { echo "no $title window"; exit 1; }

hyprctl repl "return hl.dispatch(hl.dsp.focus({ window = \"address:$addr\" }))" >/dev/null
sleep 0.5
active=$(hyprctl activewindow -j | python3 -c "import json,sys; print(json.load(sys.stdin)['address'])")
[ "$active" = "$addr" ] || { echo 'focus did not land; refusing to dispatch'; exit 1; }

floating=$(hyprctl clients -j | python3 -c "
import json, sys
addr = sys.argv[1]
print(next(c['floating'] for c in json.load(sys.stdin) if c['address'] == addr))
" "$addr")
[ "$floating" = "True" ] || hyprctl repl 'return hl.dispatch(hl.dsp.window.float())' >/dev/null
sleep 0.5
hyprctl repl 'return hl.dispatch(hl.dsp.window.move({ x = 100, y = 100 }))' >/dev/null
# The compositor's window opacity lets whatever is behind show through the
# panel, which is fine to look at and ruins a screenshot: pulteqfx shipped a
# panel shot with a terminal legible through the faceplate.
#
# It is done with a window rule and not `hyprctl setprop`, because setprop
# answers "unknown request" to every property name on this Hyprland. A rule
# registered now lands after the ones the config registered, and for opacity
# the last match wins, so this beats Omarchy's `default-opacity` tag. It
# applies to the window already mapped. The Rev F's silver panel is light
# enough that anything behind it would show.
hyprctl repl "return hl.window_rule({ match = { title = \"^($title)\$\" }, opacity = \"1 1 1\" })" >/dev/null
opacity=$(hyprctl getprop "address:$addr" opacity)
[ "$opacity" = "1" ] || { echo "window is $opacity opaque; the panel would show what is behind it"; exit 1; }
sleep 1.5

# By address again, not activewindow: moving the window can hand focus back to
# whatever was under the cursor.
read -r x y w h < <(hyprctl clients -j | python3 -c "
import json, sys
addr, title = sys.argv[1], sys.argv[2]
for c in json.load(sys.stdin):
    if c['address'] == addr:
        assert c['title'] == title, c['title']
        print(c['at'][0], c['at'][1], c['size'][0], c['size'][1])
        break
else:
    raise SystemExit('window gone')
" "$addr" "$title")
grim -g "$x,$y ${w}x${h}" "$out"
magick "$out" -format 'wrote %f, %wx%h\n' info:
