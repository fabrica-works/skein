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

# The disk floor (owners.md, 4.5): below the machine's PLAN_MIN_DISK_GB, a
# tool refuses, naming the floor, what is free and the ten largest
# directories that fill the disk, so that an owner frees them.
disk_floor() {
    local tool=$1 floor source free
    read -r floor source < <(machine_setting PLAN_MIN_DISK_GB 40)
    free=$(df -BG --output=avail "$root" | tail -n1 | tr -dc 0-9)
    ((free < floor)) || return 0
    echo "$tool: $free GB free on $root, below the disk floor of $floor GB ($source); the largest directories:" >&2
    largest_directories >&2
    return 1
}
# The repositories' targets, each directory under tmp/'s (an owner's or a
# run's worktrees and targets, one level further down under tmp/owners/),
# and the build cache's store.
largest_directories() {
    local dir dirs=()
    for dir in "$root"/*/target "$tmp"/*/* "$tmp"/owners/*/* "${XDG_CACHE_HOME:-$HOME/.cache}/kache"; do
        [[ -d $dir ]] || continue
        [[ $dir == "$tmp"/owners/* && $dir != "$tmp"/owners/*/* ]] && continue
        dirs+=("$dir")
    done
    ((${#dirs[@]})) || return 0
    du -sxh -- "${dirs[@]}" 2>/dev/null | sort -rh | head -n 10 | sed 's/^/  /'
}
