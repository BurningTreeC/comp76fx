#!/usr/bin/env bash
#
# Regenerates the rendered knobs. The renderer lives in the PultEQFx tree,
# because both panels are lit by one rig: a knob on either faceplate has to
# catch the light from the same place or the two plugins look like they were
# photographed in different rooms.
#
# Set ASSETGEN to point somewhere else if the checkout is not a sibling.
#
# The knobs are filmstrips rather than one image that gets rotated: rotating a
# sprite carries its baked lighting round with it, so the highlight would
# travel with the knob instead of staying where the panel light is. The arc and
# frame count must match SWEEP and KNOB_FRAMES in core/src/editor.

set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
assetgen="${ASSETGEN:-../pulteqfx}"

# 48 frames over the 300 degree sweep. The sweep is negative: a positive angle
# turns the part anticlockwise on screen while a knob's value grows clockwise,
# so frame 0 starts at +150 and counts down. Get the sign wrong and the strip
# runs backwards, leaving every knob resting on its maximum.
render() {
    cargo run --release --manifest-path "$assetgen/Cargo.toml" -p assetgen -- \
        --part "$1" --out "$PWD/assets/gen/$2" \
        --size "$3" --frames 48 --angle 150 --sweep -300
}

render comp76_knob       knob_large.png 208
render comp76_knob_small knob_small.png 176
