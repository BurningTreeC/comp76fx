#!/usr/bin/env bash
#
# Wraps each revision's universal plugin binary in an Audio Unit v2
# .component bundle and ad-hoc signs it. macOS only.
#
#   cargo xtask bundle-universal -p comp76fx_rev_a -p comp76fx_rev_d -p comp76fx_rev_f --release
#   tools/package_au2.sh            # all three revisions
#   tools/package_au2.sh "Rev D"    # one of them
#
# The CLAP, VST3 and AU entry points all live in the same Rust cdylib, so the
# component reuses the universal binary already in the CLAP bundle rather
# than building or lipo'ing it again.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

readonly MANUFACTURER="BrTC"
readonly COMPONENT_TYPE="aufx"

# Revision, AU subtype, bundle identifier suffix, crate, description. The
# subtypes are the ones `export_revision!` registers in each revision crate; a
# host identifies a saved plugin by them, so they never change once shipped.
readonly REVISIONS=(
    "Rev A|C76A|rev-a|comp76fx_rev_a|Bluestripe FET limiting amplifier, the original circuit"
    "Rev D|C76D|rev-d|comp76fx_rev_d|Blackface FET limiting amplifier with low noise circuitry"
    "Rev F|C76F|rev-f|comp76fx_rev_f|Blackface FET limiting amplifier with a push-pull output stage"
)

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: AUv2 bundles can only be packaged on macOS" >&2
    exit 1
fi

version="$(
    awk '
        /^\[workspace\.package\]/ { in_package=1; next }
        /^\[/ && in_package { exit }
        in_package && /^version[[:space:]]*=/ {
            gsub(/^[^"]*"/, "")
            gsub(/".*$/, "")
            print
            exit
        }
    ' Cargo.toml
)"

if [[ -z "${version}" ]]; then
    echo "error: could not determine the version from Cargo.toml" >&2
    exit 1
fi

package() {
    local revision="$1" subtype="$2" id="$3" crate="$4" description="$5"
    local plugin="Comp76Fx ${revision}"

    local clap_binary="target/bundled/${plugin}.clap/Contents/MacOS/${plugin}"
    local component="target/bundled/${plugin}.component"
    local contents="${component}/Contents"
    local component_binary="${contents}/MacOS/${plugin}"

    if [[ ! -f "${clap_binary}" ]]; then
        echo "error: universal CLAP binary does not exist:" >&2
        echo "  ${clap_binary}" >&2
        echo "Run 'cargo xtask bundle-universal -p comp76fx_rev_a -p comp76fx_rev_d -p comp76fx_rev_f --release' first." >&2
        exit 1
    fi

    echo "Checking ${plugin}..."
    local archs
    archs="$(lipo -archs "${clap_binary}")"
    echo "Architectures: ${archs}"
    if [[ "${archs}" != *"x86_64"* || "${archs}" != *"arm64"* ]]; then
        echo "error: ${clap_binary} is not a universal x86_64 + arm64 binary" >&2
        exit 1
    fi

    # The AU adapter must actually have been linked into the plugin.
    local symbols
    symbols="$(nm -gU "${clap_binary}")"
    for symbol in NiceAu2Factory nice_au2_register_plugin_entry; do
        if ! grep -q "${symbol}" <<<"${symbols}"; then
            echo "error: ${symbol} is missing from ${clap_binary}" >&2
            echo "The nice-plug-au2 adapter was not linked into ${plugin}." >&2
            exit 1
        fi
    done
    # And so must this revision's own Cocoa view factory, or the host finds
    # no editor. Its name is only in the binary if the class is: nothing in
    # Rust spells it, and the class is not an exported symbol nm could show.
    #
    # The strings are collected first rather than piped into `grep -q`. That
    # stops reading at the first match, `strings` then fails writing the rest,
    # and under pipefail a class that was found reads as missing -- which is
    # how the first macOS build failed.
    local factory="NiceAu2CocoaViewFactory_${crate}" names
    names="$(strings -a "${clap_binary}")"
    if ! grep -qxF "${factory}" <<<"${names}"; then
        echo "error: the Cocoa view factory ${factory} is missing from ${clap_binary}" >&2
        exit 1
    fi

    echo "Creating ${component}..."
    rm -rf "${component}"
    mkdir -p "${contents}/MacOS" "${contents}/Resources"
    cp "${clap_binary}" "${component_binary}"
    chmod +x "${component_binary}"

    cat > "${contents}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>

    <key>CFBundleExecutable</key>
    <string>${plugin}</string>

    <key>CFBundleIdentifier</key>
    <string>com.burningtreec.comp76fx.${id}.au</string>

    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>

    <key>CFBundleName</key>
    <string>${plugin}</string>

    <key>CFBundleDisplayName</key>
    <string>${plugin}</string>

    <key>CFBundlePackageType</key>
    <string>BNDL</string>

    <key>CFBundleShortVersionString</key>
    <string>${version}</string>

    <key>CFBundleVersion</key>
    <string>${version}</string>

    <key>NSHumanReadableCopyright</key>
    <string>Copyright BurningTreeC</string>

    <key>AudioComponents</key>
    <array>
        <dict>
            <key>type</key>
            <string>${COMPONENT_TYPE}</string>

            <key>subtype</key>
            <string>${subtype}</string>

            <key>manufacturer</key>
            <string>${MANUFACTURER}</string>

            <key>name</key>
            <string>BurningTreeC: ${plugin}</string>

            <key>description</key>
            <string>${description}</string>

            <key>factoryFunction</key>
            <string>NiceAu2Factory</string>

            <!-- Must match the version ClassInfo reports; see
                 vendor/nice-plug-au2/src/bridge/properties.rs. -->
            <key>version</key>
            <integer>1</integer>

            <key>sandboxSafe</key>
            <true/>
        </dict>
    </array>
</dict>
</plist>
EOF

    plutil -lint "${contents}/Info.plist"

    echo "Ad-hoc signing ${component}..."
    codesign --force --deep --sign - "${component}"
    codesign --verify --deep --strict --verbose=2 "${component}"

    echo "Created ${component} (${COMPONENT_TYPE} ${subtype} ${MANUFACTURER})"
    echo
}

found=0
for entry in "${REVISIONS[@]}"; do
    IFS='|' read -r revision subtype id crate description <<<"${entry}"
    if [[ $# -gt 0 && "$1" != "${revision}" ]]; then
        continue
    fi
    package "${revision}" "${subtype}" "${id}" "${crate}" "${description}"
    found=1
done

if [[ "${found}" -eq 0 ]]; then
    echo "error: no revision called '$1'; expected Rev A, Rev D or Rev F" >&2
    exit 1
fi
