# shellcheck shell=bash
# Gate runtime and evidence identity (gate-policy.md, 1 to 3).
# Sourced by land and gate admission; no environment/configuration dump.

_gate_policy_script=$(readlink -f "${BASH_SOURCE[0]}")

gate_policy_prepare() {
    local prepared
    unset SKEIN_GATE_POLICY_ID
    prepared=$(python3 -I -B - "$_gate_policy_script" <<'PY'
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import subprocess
import sys
import tomllib

PIN = "147.0.7727.101"
SCRIPT = Path(sys.argv[1])
CWD = Path.cwd()


def fail(message):
    raise ValueError(message)


def digest_file(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def file_identity(path):
    resolved = path.resolve(strict=True)
    return {"path": str(path.absolute()), "resolved": str(resolved), "sha256": digest_file(resolved)}


def executable(value, base=CWD, environment=None):
    if not isinstance(value, str) or not value:
        fail("invalid executable selection")
    if "/" in value:
        path = Path(value)
        if not path.is_absolute():
            path = base / path
    else:
        found = shutil.which(value, path=environment.get("PATH") if environment is not None else None)
        if not found:
            fail("a declared executable is unavailable")
        path = Path(found)
    path = path.absolute()
    if not path.is_file() or not os.access(path, os.X_OK):
        fail("a declared executable is not executable")
    return path


def target_command_identity(value, base, environment):
    # Targets not used by this gate may name tools absent on this machine.
    # Cargo decides whether they are needed; their availability is evidence.
    try:
        command = value[0] if isinstance(value, list) else value.split()[0]
        return file_identity(executable(command, base, environment))
    except (OSError, ValueError, IndexError):
        return {"unavailable": True}


def output(command, environment=None):
    result = subprocess.run(command, env=environment, capture_output=True, timeout=15)
    if result.returncode:
        fail("a declared runtime version check failed")
    return result.stdout.decode("utf-8", errors="strict").strip()


def semantic_name(name):
    return name in {
        "PATH", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN", "RUSTC_BOOTSTRAP", "CARGO_BUILD_TARGET",
        "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC", "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTFLAGS",
        "CARGO_BUILD_RUSTDOCFLAGS", "CARGO_BUILD_INCREMENTAL", "CARGO_INCREMENTAL",
    } or re.fullmatch(r"CARGO_PROFILE_[A-Z0-9_]+_(OPT_LEVEL|DEBUG|SPLIT_DEBUGINFO|STRIP|DEBUG_ASSERTIONS|OVERFLOW_CHECKS|LTO|PANIC|INCREMENTAL|CODEGEN_UNITS|RPATH)", name) is not None or re.fullmatch(
        r"CARGO_TARGET_[A-Z0-9_]+_(RUSTFLAGS|RUSTDOCFLAGS|LINKER|RUNNER)", name
    ) is not None


def merge(left, right):
    for key, value in right.items():
        if key in left and isinstance(left[key], dict) and isinstance(value, dict):
            merge(left[key], value)
        elif key in left and isinstance(left[key], list) and isinstance(value, list):
            left[key] += value
        else:
            left[key] = deepcopy(value)


def main():
    installation = Path.home() / ".local/opt" / f"chromium-{PIN}"
    launcher = installation / "bin/chromium"
    runtime = executable(os.environ.get("SKEIN_TEST_CHROMIUM", str(launcher))).resolve(strict=True)
    # The known kit package has a launcher that sources machine settings.
    # Execute its packaged binary directly and identify both files.
    packaged = installation / "usr/lib/chromium/chromium"
    if launcher.exists() and runtime == launcher.resolve() and packaged.is_file():
        runtime = executable(str(packaged)).resolve(strict=True)
    version = output([str(runtime), "--version"])
    if re.fullmatch(r"Chromium " + re.escape(PIN) + r"(?:\s+.*)?", version) is None:
        fail(f"Chromium {PIN} is required; the selected runtime reports another version")
    browser = {"executable": file_identity(runtime), "version": version}
    if packaged.exists() and runtime == packaged.resolve() and launcher.is_file():
        browser["kit_launcher"] = file_identity(launcher)
    resources = []
    if runtime.parent.name == "chromium":
        for path in sorted(runtime.parent.rglob("*")):
            if path.is_file() and path != runtime:
                resources.append([str(path.relative_to(runtime.parent)), file_identity(path)])
    browser["resources"] = resources

    fonts = Path(os.environ.get("SKEIN_TEST_FONTS", "/usr/share/fonts/truetype/dejavu")).resolve(strict=True)
    if not (fonts / "DejaVuSans.ttf").is_file():
        fail("the declared fixture font DejaVuSans.ttf is unavailable")
    font_files = [[str(path.relative_to(fonts)), file_identity(path)]
                  for path in sorted(fonts.rglob("*"))
                  if path.is_file() and path.suffix.lower() in {".ttf", ".otf", ".woff", ".woff2"}]

    config = {}
    origins = {}
    config_records = []
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
    directories = [cargo_home] + [parent / ".cargo" for parent in reversed([CWD, *CWD.parents])]
    seen = set()
    build_fields = {"rustc", "rustdoc", "rustc-wrapper", "rustc-workspace-wrapper",
                    "target", "rustflags", "rustdocflags", "incremental"}
    target_fields = {"rustflags", "rustdocflags", "linker", "runner"}
    for directory in directories:
        path = directory / "config"
        if not path.is_file():
            path = directory / "config.toml"
        if not path.is_file() or path.resolve() in seen:
            continue
        seen.add(path.resolve())
        with path.open("rb") as source:
            parsed = tomllib.load(source)
        if "include" in parsed:
            fail("Cargo configuration includes are outside the declared policy")
        selected = {"build": {key: value for key, value in parsed.get("build", {}).items() if key in build_fields},
                    "target": {target: {key: value for key, value in fields.items() if key in target_fields}
                               for target, fields in parsed.get("target", {}).items()},
                    "env": {key: value for key, value in parsed.get("env", {}).items() if semantic_name(key)}}
        for key in ("profile", "unstable"):
            if key in parsed:
                selected[key] = parsed[key]
        config_records.append([str(path.resolve()), selected])
        merge(config, selected)
        for key in selected["build"]:
            origins[("build", key)] = directory.parent
        for target, fields in selected["target"].items():
            for key in fields:
                origins[(target, key)] = directory.parent
        for key in selected["env"]:
            origins[("env", key)] = directory.parent

    environment = dict(os.environ)
    for name, value in config.get("env", {}).items():
        if isinstance(value, str):
            value = {"value": value}
        if name not in environment or value.get("force", False):
            setting = value["value"]
            if value.get("relative", False):
                setting = str((origins[("env", name)] / setting).resolve())
            environment[name] = setting
    build = config.get("build", {})
    tools = {"gate-shell": file_identity(executable("bash"))}
    gate_command = os.environ.get("SKEIN_GATE_COMMAND", "scripts/check.sh")
    gate_words = shlex.split(gate_command)
    gate_executable = target_command_identity(gate_words[:1], CWD, os.environ)
    # Cargo's dedicated variable precedes its generic config variable.
    for field, dedicated, fallback in (
        ("rustc", "RUSTC", "rustc"), ("rustdoc", "RUSTDOC", "rustdoc"),
        ("rustc-wrapper", "RUSTC_WRAPPER", ""),
        ("rustc-workspace-wrapper", "RUSTC_WORKSPACE_WRAPPER", ""),
    ):
        generic = "CARGO_BUILD_" + field.upper().replace("-", "_")
        value = environment.get(dedicated, environment.get(generic, build.get(field, fallback)))
        base = CWD if dedicated in environment or generic in environment else origins.get(("build", field), CWD)
        tools[field] = file_identity(executable(value, base, environment)) if value else None
    for target, fields in config.get("target", {}).items():
        for field in ("linker", "runner"):
            if field in fields:
                value = fields[field]
                tools[f"target.{target}.{field}"] = target_command_identity(value, origins[(target, field)], environment)
    for name, value in environment.items():
        if semantic_name(name) and re.fullmatch(r"CARGO_TARGET_[A-Z0-9_]+_(LINKER|RUNNER)", name):
            tools[name] = target_command_identity(value, CWD, environment)
    cargo = executable("cargo")
    tools["cargo"] = file_identity(cargo)
    rustc = tools["rustc"]["path"]
    # Resolve installed rustup tools before executing shims, so validation
    # cannot install a missing toolchain from a rust-toolchain selection.
    rustup = shutil.which("rustup", path=environment.get("PATH"))
    if not rustup and Path(rustc).resolve().name == "rustup":
        rustup = str(Path(rustc).resolve())
    if rustup:
        for name in ("rustc", "rustdoc", "cargo"):
            invocation = Path(tools[name]["path"])
            if invocation.resolve() == Path(rustup).resolve():
                selected = output([rustup, "which", name], environment)
                tools["rustup-" + name] = file_identity(Path(selected))
    compiler_version = output([rustc, "-vV"], environment)
    implementations = {}
    for path in [SCRIPT, SCRIPT.parent / "land", SCRIPT.parent / "heavy", SCRIPT.parent / "heavy-recovery.py", SCRIPT.parent / "kit.sh", CWD / "scripts/check.sh"]:
        implementations[path.name] = digest_file(path) if path.is_file() else None
    record = {"schema": "skein-gate-policy-v1", "chromium": browser,
              "fonts": {"path": str(fonts), "files": font_files},
              "fixture_contract": "browser DejaVuSans.ttf; repository fixtures by Gate-Tree; UI bundle built by gate",
              "tools": tools, "gate_command": gate_command, "gate_executable": gate_executable, "rustc": compiler_version, "cargo": output([str(cargo), "-V"], environment),
              "environment": {name: value for name, value in environment.items() if semantic_name(name)},
              "cargo_config": config_records, "implementations": implementations}
    identity = "sha256:" + hashlib.sha256(json.dumps(record, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if any("\n" in str(path) or "\t" in str(path) for path in (runtime, fonts)):
        fail("runtime paths must be single-line paths")
    print(identity + "\t" + str(runtime) + "\t" + str(fonts))


try:
    main()
except (OSError, ValueError, KeyError, TypeError, IndexError, AttributeError, RuntimeError, subprocess.SubprocessError):
    # Arbitrary runtime output, flags and configuration values stay private.
    print("gate-policy: cannot validate the declared runtime/toolchain/configuration; Chromium " + PIN + " and DejaVuSans.ttf are required", file=sys.stderr)
    sys.exit(2)
PY
) || return 2
    IFS=$'\t' read -r SKEIN_GATE_POLICY_ID SKEIN_TEST_CHROMIUM SKEIN_TEST_FONTS <<<"$prepared"
    SKEIN_GATE_POLICY_HELPER=$_gate_policy_script
    export SKEIN_GATE_POLICY_ID SKEIN_TEST_CHROMIUM SKEIN_TEST_FONTS SKEIN_GATE_POLICY_HELPER
}

gate_policy_identity() (
    gate_policy_prepare || exit "$?"
    printf '%s\n' "$SKEIN_GATE_POLICY_ID"
)
