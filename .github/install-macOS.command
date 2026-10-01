#!/usr/bin/env bash
#
# Installs the Comp76Fx plugins for the current user and clears the macOS quarantine flag,
# which is otherwise what stops an ad-hoc signed plugin from loading.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# A folder named after the vendor, matching the Linux and Windows installers.
# Audio Units have no such convention: hosts only look in Components itself.
readonly VENDOR="BurningTreeC"
clap_dir="$HOME/Library/Audio/Plug-Ins/CLAP/$VENDOR"
vst3_dir="$HOME/Library/Audio/Plug-Ins/VST3/$VENDOR"
au_dir="$HOME/Library/Audio/Plug-Ins/Components"

install_bundle() {
    local bundle="$1" dest="$2"
    [ -e "$bundle" ] || { echo "missing $bundle, is this archive complete?"; exit 1; }
    mkdir -p "$dest"
    rm -rf "${dest:?}/$bundle"
    cp -R "$bundle" "$dest/"
    # Signed ad-hoc rather than notarized, so the quarantine flag has to go.
    xattr -dr com.apple.quarantine "$dest/$bundle" 2>/dev/null || true
    echo "Installed $bundle to $dest"
}

for revision in "Rev A" "Rev D" "Rev F"; do
    install_bundle "Comp76Fx $revision.clap" "$clap_dir"
    install_bundle "Comp76Fx $revision.vst3" "$vst3_dir"
    install_bundle "Comp76Fx $revision.component" "$au_dir"
done

# Make macOS forget what it cached about Audio Units, so a host sees these
# ones. The service restarts on its own when a host next asks for it.
killall -9 AudioComponentRegistrar 2>/dev/null || true

echo
echo "Done. Rescan plugins in your DAW."
