# shellcheck shell=bash
# shellcheck disable=SC2154 # callers set root and tmp
# Functions shared by land, heavy, setup and kache-setup once they know
# the workspace; sourced, not run.

# The machine file accepts only these five integer settings. An environment
# value wins, then the file's last valid value, then the caller's default.
machine_setting() {
    local key=$1 fallback=$2 value
    if [[ -v $key ]]; then
        printf '%s environment\n' "${!key}"
        return
    fi
    value=$(awk -F= -v key="$key" '
        /^(PLAN_MIN_MEM_GB|PLAN_MIN_DISK_GB|HEAVY_BUILD_SLOTS|HEAVY_BUILD_JOBS|KACHE_MAX_GB)=[0-9]+$/ && $1 == key { value = $2 }
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
# The directories under tmp/ (worktrees and their targets, and those one
# level further down under tmp/owners/), the repositories' targets, and
# the build cache's store.
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
