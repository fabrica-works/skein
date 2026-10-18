# shellcheck shell=bash
# shellcheck disable=SC2154 # plan, land and heavy set kit, root and tmp
# The run's kit (development.md, section 3.6). Sourced by plan, land and
# heavy, once they know the workspace, not run.
#
# A run takes its kit from its claim: plan claim adds kit/ to the run's
# directory, a worktree of skein at the commit it was claimed at. While it
# is this machine's run, a tool run from any other copy of the kit, the one
# linked on PATH among them, hands over to that worktree's, naming itself
# in DEV_KIT_HOME for plan check. A run claimed without kit/ uses the copy
# it is run from.
run_kit() {
    local tool=$1 name pinned
    shift
    [[ -f $tmp/active-plan ]] || return 0
    IFS= read -r name <"$tmp/active-plan" || :
    pinned=$tmp/$name/kit/scripts/dev
    [[ -x $pinned/$tool ]] || return 0
    pinned=$(readlink -f "$pinned")
    [[ $pinned != "$kit" ]] || return 0
    DEV_ROOT=$root DEV_TMP=$tmp DEV_KIT_HOME=$kit exec "$pinned/$tool" "$@"
}

# The machine file accepts only these four integer settings. An environment
# value wins, then the file's last valid value, then the caller's default.
machine_setting() {
    local key=$1 fallback=$2 value
    if [[ -v $key ]]; then
        printf '%s environment\n' "${!key}"
        return
    fi
    value=$(awk -F= -v key="$key" '
        /^(PLAN_MIN_MEM_GB|PLAN_MIN_DISK_GB|HEAVY_BUILD_SLOTS|HEAVY_BUILD_JOBS)=[0-9]+$/ && $1 == key { value = $2 }
        END { print value }' "$tmp/machine.env" 2>/dev/null || true)
    if [[ -n $value ]]; then printf '%s machine.env\n' "$value"; else printf '%s default\n' "$fallback"; fi
}
