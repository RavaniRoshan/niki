#!/usr/bin/env bash
# Remove what the installer put there, and nothing else.
#
#   ./scripts/uninstall.sh              # remove the binary, keep your data
#   ./scripts/uninstall.sh --purge      # also remove config, history, keys
#   ./scripts/uninstall.sh --dry-run    # print every path, delete nothing
#
# The rule that shapes this file: **the default removes files and leaves data.** A user's task
# history, run reports, memory and API keys are not the installer's to delete, and an uninstall
# that removes them is a bug report. `--purge` is the way to ask for that, out loud.
#
# It is also idempotent, because an uninstall that fails the second time is worse than one that
# does nothing: running this twice must succeed twice.

set -uo pipefail

PURGE=0
DRY_RUN=0

while [ $# -gt 0 ]; do
    case "$1" in
        --purge) PURGE=1; shift ;;
        --dry-run) DRY_RUN=1; shift ;;
        -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "uninstall: unknown flag: $1" >&2; exit 2 ;;
    esac
done

# The same ladder install.sh uses to choose a destination, in the same order. Resolving it the
# same way is what makes this an uninstall *of that installer* rather than a guess.
if [ -n "${NIKI_INSTALL_DIR:-}" ]; then
    DEST="$NIKI_INSTALL_DIR"
elif [ -n "${XDG_BIN_DIR:-}" ]; then
    DEST="$XDG_BIN_DIR"
elif [ -d "$HOME/.local/bin" ]; then
    DEST="$HOME/.local/bin"
else
    DEST="$HOME/.niki/bin"
fi

# Data lives here regardless of where the binary went. Listed even in the default run so the
# user is told what is being kept, rather than finding out later.
DATA_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/niki"
STATE_DIR="$HOME/.niki"

removed=0
kept=0

# A path is only ever removed when it is one of ours. The most expensive mistake available here
# is expanding a variable that was empty and deleting a directory named after it, so every
# removal goes through `safe_rm`, which refuses anything relative, anything that resolves to `/`,
# and anything that does not exist.
safe_rm() {
    local target="$1" label="$2"
    case "$target" in
        ""|"/"|"."|"..")
            echo "  REFUSED  ${label}: '${target}' is not a path worth deleting" >&2
            return 1
            ;;
    esac
    if [ -e "$target" ] || [ -L "$target" ]; then
        if [ "$DRY_RUN" -eq 1 ]; then
            echo "  would remove  ${label}: ${target}"
        else
            rm -rf -- "$target" || {
                echo "  FAILED   ${label}: ${target} could not be removed" >&2
                return 1
            }
            echo "  removed       ${label}: ${target}"
        fi
        removed=$((removed + 1))
        return 0
    fi
    echo "  absent        ${label}: ${target}"
    return 0
}

keep() {
    kept=$((kept + 1))
    if [ "$DRY_RUN" -eq 1 ]; then
        echo "  would keep    ${1}: ${2}"
    else
        echo "  kept          ${1}: ${2}"
    fi
}

echo "niki uninstall"
echo

# One guard, before any target is built. An install dir of `/` or `` would otherwise turn the
# removals below into a recursive delete of something enormous, and the per-path check cannot
# catch it because each individual target still looks like a plausible path.
case "$DEST" in
    ""|"/")
        echo "REFUSED: NIKI_INSTALL_DIR/XDG_BIN_DIR resolved to '${DEST}', which is not a" >&2
        echo "         directory this script will delete from. Nothing was removed." >&2
        echo "         Set NIKI_INSTALL_DIR to the directory install.sh used." >&2
        exit 2
        ;;
esac

# ── files the installer created ─────────────────────────────────────────────
echo "Installed files"
safe_rm "$DEST/niki" "engine"
safe_rm "$DEST/niki-shell" "interface"
# cargo-dist's `install-updater = true` drops an updater next to the binary.
safe_rm "$DEST/niki-update" "updater"
echo

# A PATH entry pointing at a directory this script just emptied is worse than no entry at all,
# because every later `niki` on that PATH fails with "command not found". Say so plainly rather
# than editing the user's shell profile, which install.sh also refuses to touch.
case ":$PATH:" in
    *":$DEST:"*)
        echo "NOTE: ${DEST} is on your PATH and is now empty of niki."
        echo "      Remove it from your shell profile if nothing else lives there:"
        echo "        sed -i '/${DEST}/d' ~/.bashrc   # or ~/.zshrc"
        echo
        ;;
esac

# ── data ─────────────────────────────────────────────────────────────────────
echo "Your data"
if [ "$PURGE" -eq 1 ]; then
    safe_rm "$DATA_DIR" "config, keys and history"
    safe_rm "$STATE_DIR" "state and logs"
else
    keep "config, keys, history" "$DATA_DIR"
    keep "state and logs" "$STATE_DIR"
    echo
    echo "  Pass --purge to remove these too. Until then they are left exactly as they are."
fi
echo

if [ "$DRY_RUN" -eq 1 ]; then
    echo "dry run: nothing was deleted."
    exit 0
fi

echo "Done. ${removed} path(s) processed, ${kept} kept."
if [ "$PURGE" -eq 0 ]; then
    echo "Your configuration, task history and API keys were NOT removed."
fi
exit 0