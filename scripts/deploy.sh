#!/usr/bin/env bash
# Install uniloader, its overlay, this adapter and the address database into DayZ.
#
#   scripts/deploy.sh            install (build first: ../../uniloader/scripts/gate.sh and
#                                scripts/gate.sh)
#   scripts/deploy.sh --check    say what would be installed, change nothing
#
# uniloader goes in as dxgi.dll, the name the game loads it under. A dxgi.dll that is not
# uniloader (the old dayz-plugin-loader, ReShade, ...) is moved to dxgi.dll.before-uniloader
# once and never overwritten after that. Plugin DLLs in plugins/ that are not uniloader plugins
# are left where they are: uniloader ignores them.
#
# Env: DAYZ_DIR (default: the Steam library on the data drive).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UNILOADER="$ROOT/../../uniloader"
TARGET=x86_64-pc-windows-msvc
DAYZ="${DAYZ_DIR:-/run/media/system/Data/Games/Steam/steamapps/common/DayZ}"
DATA_REPO="$ROOT/../dayz-data"

CHECK=0
case "${1:-}" in
    --check) CHECK=1 ;;
    "") ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
esac

die() { echo "error: $*" >&2; exit 1; }

[[ -f "$DAYZ/DayZ_x64.exe" ]] || die "no DayZ_x64.exe in $DAYZ; set DAYZ_DIR"
if pgrep -f 'DayZ_x6[4]\.exe' > /dev/null; then
    die "DayZ is running; a loaded DLL cannot be replaced"
fi

loader="$UNILOADER/target/$TARGET/release/uniloader.dll"
overlay="$UNILOADER/target/$TARGET/release/overlay.dll"
adapter="$ROOT/target/$TARGET/release/dayz.dll"
for file in "$loader" "$overlay" "$adapter"; do
    [[ -f "$file" ]] || die "not built: $file"
done

# Where the database comes from: a dayz-data checkout beside this one, else the copy the old
# loader already installed. The adapter also finds the old copy on its own.
data_source=""
if [[ -f "$DATA_REPO/patterns.json" ]]; then
    data_source="$DATA_REPO"
elif [[ -f "$DAYZ/plugin-loader/data/patterns.json" ]]; then
    data_source="$DAYZ/plugin-loader/data"
fi

echo "install into $DAYZ:"
echo "  dxgi.dll                 <- $loader"
echo "  plugins/overlay.dll      <- $overlay"
echo "  plugins/dayz.dll         <- $adapter"
echo "  plugins/dayz-data/       <- ${data_source:-nothing found}"
if [[ -f "$DAYZ/dxgi.dll" ]] && ! grep -qa 'uniloader_plugin_describe' "$DAYZ/dxgi.dll"; then
    echo "  the current dxgi.dll is not uniloader; it moves to dxgi.dll.before-uniloader"
fi
(( CHECK )) && exit 0

if [[ -f "$DAYZ/dxgi.dll" ]] && ! grep -qa 'uniloader_plugin_describe' "$DAYZ/dxgi.dll"; then
    [[ -e "$DAYZ/dxgi.dll.before-uniloader" ]] \
        || mv -- "$DAYZ/dxgi.dll" "$DAYZ/dxgi.dll.before-uniloader"
fi
mkdir -p "$DAYZ/plugins"
install -m 0644 "$loader" "$DAYZ/dxgi.dll"
install -m 0644 "$overlay" "$DAYZ/plugins/overlay.dll"
install -m 0644 "$adapter" "$DAYZ/plugins/dayz.dll"
if [[ -n "$data_source" ]]; then
    mkdir -p "$DAYZ/plugins/dayz-data/builds"
    install -m 0644 "$data_source/patterns.json" "$DAYZ/plugins/dayz-data/patterns.json"
    for build in "$data_source"/builds/*.json; do
        install -m 0644 "$build" "$DAYZ/plugins/dayz-data/builds/"
    done
fi
echo "deployed; start the game with scripts/run-dayz.sh"
