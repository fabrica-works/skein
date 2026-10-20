"""Canonical JSON subset and tree identities (lab.md, 2.2)."""
import hashlib
import json
import re
from .records import FORMATS, SAFE, Refused, digest_value, relative


def _check(value, member="$", stack=None):
    if stack is None:
        stack = set()
    if value is None or type(value) is bool:
        return
    if type(value) is int:
        if not -SAFE <= value <= SAFE:
            raise Refused(member, "integer outside the exact safe range")
    elif type(value) is str:
        try:
            value.encode("utf-8", "strict")
        except UnicodeError as error:
            raise Refused(member, "string is not Unicode scalar text") from error
    elif type(value) in (dict, list):
        if id(value) in stack:
            raise Refused(member, "cyclic value")
        stack.add(id(value))
        if type(value) is dict:
            for key, item in value.items():
                if type(key) is not str or not key.isascii():
                    raise Refused(member, "object keys must be ASCII strings")
                _check(item, member + "." + key, stack)
        else:
            for index, item in enumerate(value):
                _check(item, f"{member}[{index}]", stack)
        stack.remove(id(value))
    else:
        raise Refused(member, "expected null, boolean, safe integer, string, array or object")


def dumps(value):
    _check(value)
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"),
                      allow_nan=False).encode("utf-8", "strict")


def record_bytes(value):
    if type(value) is not dict:
        raise Refused("$", "a record must be an object")
    return dumps(value) + b"\n"


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refused(key, "duplicate member")
        result[key] = value
    return result


def _no_number(text):
    raise Refused("$", "floats and nonfinite numbers are forbidden")


def loads(data):
    if type(data) is not bytes:
        raise Refused("$", "expected record bytes")
    try:
        value = json.loads(data.decode("utf-8", "strict"), object_pairs_hook=_pairs,
                           parse_float=_no_number, parse_constant=_no_number)
    except (UnicodeError, json.JSONDecodeError, RecursionError, ValueError) as error:
        if isinstance(error, Refused):
            raise
        raise Refused("$", "invalid JSON or UTF-8") from error
    if type(value) is not dict:
        raise Refused("$", "a record must be an object")
    if record_bytes(value) != data:
        raise Refused("$", "record is not canonical JSON with one final newline")
    if type(value.get("format")) is not str or value["format"] not in FORMATS:
        raise Refused("format", "unknown record format")
    return value


def digest(value):
    if type(value) is not dict:
        raise Refused("$", "a record must be an object")
    return "sha256:" + hashlib.sha256(dumps({k: v for k, v in value.items() if k != "about"})).hexdigest()


def manifest(entries):
    result = []
    paths = set()
    for entry in entries:
        if type(entry) is not dict or set(entry) - {"path", "bytes", "sha256", "exec", "kind"}:
            raise Refused("entries", "expected regular-file metadata")
        if entry.get("kind", "file") != "file":
            raise Refused("entries", "links and special files are forbidden")
        path = entry.get("path")
        if not relative(path):
            raise Refused("entries.path", "expected a contained relative path")
        _check(path)
        if path in paths:
            raise Refused(path, "duplicate path")
        paths.add(path)
        if type(entry.get("bytes")) is not int or not 0 <= entry["bytes"] <= SAFE:
            raise Refused(path, "invalid file size")
        if type(entry.get("exec")) is not bool or not digest_value(entry.get("sha256")):
            raise Refused(path, "invalid digest or execute bit")
        result.append({key: entry[key] for key in ("path", "bytes", "sha256", "exec")})
    if any("/".join(path.split("/")[:i]) in paths
           for path in paths for i in range(1, len(path.split("/")))):
        raise Refused("entries.path", "a file is also a parent directory")
    return {"format": "lab-tree/1", "entries": sorted(result, key=lambda e: e["path"].encode("utf-8"))}


def tree_digest(entries):
    return digest(manifest(entries))


def u64(text):
    if type(text) is not str or not re.fullmatch(r"0|[1-9][0-9]{0,19}", text):
        raise Refused("seed", "expected a canonical unsigned decimal string")
    value = int(text)
    if value > (1 << 64) - 1:
        raise Refused("seed", "unsigned 64-bit value out of range")
    return value


def u64_text(value):
    if type(value) is not int or not 0 <= value < (1 << 64):
        raise Refused("seed", "expected an unsigned 64-bit integer")
    return str(value)
