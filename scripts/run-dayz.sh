#!/usr/bin/env bash
# Start DayZ with uniloader, through the Steam Linux Runtime and Proton, skipping the DayZ
# Launcher. Steam must be running for DRM, but Steam's launch options are not used: the
# environment, the DLL override and the command line all come from here.
#
#   scripts/run-dayz.sh                 start, with every plugin in plugins/
#   scripts/run-dayz.sh --console       also open uniloader's console window
#   scripts/run-dayz.sh --log           follow the loader log instead of starting anything
#   scripts/run-dayz.sh --stop          close the game
#   scripts/run-dayz.sh -- ARGS         extra arguments for the game itself
#
# The game runs detached. The loader writes <game>/uniloader/logs/loader.log; this script's
# own output and the game's stray stdout go to build/logs/dayz.log, and the Proton log to
# build/logs/steam-221100.log.
#
# Environment overrides: DAYZ_DIR, STEAM_LIBRARY, STEAM_ROOT, PROTON_DIR, SLR_DIR.
set -euo pipefail
IFS=$'\n\t'

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(dirname -- "$script_dir")"
log_dir="$repo_root/build/logs"
app_id=221100
steam_root="${STEAM_ROOT:-$HOME/.local/share/Steam}"
steam_library="${STEAM_LIBRARY:-/run/media/system/Data/Games/Steam}"
dayz_dir="${DAYZ_DIR:-$steam_library/steamapps/common/DayZ}"
proton_dir="${PROTON_DIR:-$steam_root/compatibilitytools.d/Proton-GE Latest}"
# GE-Proton 11 requires the Steam Linux Runtime 4.0 container; Proton 8/9 use sniper.
slr_dir="${SLR_DIR:-$steam_library/steamapps/common/SteamLinuxRuntime_4}"
compat_data="$steam_library/steamapps/compatdata/$app_id"
loader_log="$dayz_dir/uniloader/logs/loader.log"

say() { printf '==> %s\n' "$*"; }
die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# -nobe: no BattlEye client, which the launcher would otherwise start.
game_args=(-nobe)
while (($#)); do
    case "$1" in
        --console) game_args+=(--loader-console) ;;
        --stop)
            pkill -f 'DayZ_x64\.exe' && say "asked DayZ to close" || say "DayZ is not running"
            exit 0
            ;;
        --log)
            [[ -f "$loader_log" ]] || die "no loader log yet at $loader_log"
            exec tail -n 200 -f "$loader_log"
            ;;
        --)
            shift
            game_args+=("$@")
            break
            ;;
        -h | --help)
            sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

[[ -f "$dayz_dir/DayZ_x64.exe" ]] || die "DayZ not found in $dayz_dir"
grep -qa uniloader_plugin_describe "$dayz_dir/dxgi.dll" 2> /dev/null \
    || die "uniloader is not deployed as dxgi.dll; run scripts/deploy.sh"
[[ -x "$proton_dir/proton" ]] || die "Proton not found: $proton_dir"
[[ -x "$slr_dir/_v2-entry-point" ]] || die "Steam Linux Runtime not found: $slr_dir"
[[ -d "$compat_data/pfx" ]] || die "Proton prefix missing: $compat_data/pfx"
pgrep -x steam > /dev/null || die "the Steam client is not running; DayZ needs it for DRM"
if pgrep -f 'DayZ_x6[4]\.exe|DayZLaunche[r]\.exe' > /dev/null; then
    die "DayZ is already running; stop it with $0 --stop"
fi
mkdir -p "$log_dir"

# Say what will be loaded: a DLL in plugins/ is only a plugin when it exports
# uniloader_plugin_describe; anything else there is ignored.
say "plugins in $dayz_dir/plugins:"
shopt -s nullglob
for dll in "$dayz_dir"/plugins/*.dll; do
    if grep -qa uniloader_plugin_describe "$dll"; then
        printf '    %-24s plugin\n' "$(basename -- "$dll")"
    else
        printf '    %-24s not a uniloader plugin, ignored\n' "$(basename -- "$dll")"
    fi
done
shopt -u nullglob

[[ -f "$log_dir/dayz.log" ]] && mv -f -- "$log_dir/dayz.log" "$log_dir/dayz.prev.log"

env_list=(
    "STEAM_COMPAT_APP_ID=$app_id"
    "SteamAppId=$app_id"
    "SteamGameId=$app_id"
    "STEAM_COMPAT_CLIENT_INSTALL_PATH=$steam_root"
    "STEAM_COMPAT_DATA_PATH=$compat_data"
    "STEAM_COMPAT_INSTALL_PATH=$dayz_dir"
    "STEAM_COMPAT_LIBRARY_PATHS=$steam_library:$steam_root"
    "STEAM_COMPAT_TOOL_PATHS=$proton_dir:$slr_dir"
    "PROTON_LOG=1"
    "PROTON_LOG_DIR=$log_dir"
    # The whole point: the game must prefer uniloader's dxgi.dll over the system one.
    "WINEDLLOVERRIDES=dxgi=n,b"
)

say "game arguments: ${game_args[*]}"
say "starting DayZ_x64.exe via $(basename -- "$proton_dir")"
say "loader log: $loader_log  (follow it with $0 --log)"
cd "$dayz_dir"
setsid env "${env_list[@]}" "$slr_dir/_v2-entry-point" --verb=waitforexitandrun -- \
    "$proton_dir/proton" waitforexitandrun "$dayz_dir/DayZ_x64.exe" "${game_args[@]}" \
    > "$log_dir/dayz.log" 2>&1 < /dev/null &
say "launch requested (pid $!); close it with $0 --stop"
