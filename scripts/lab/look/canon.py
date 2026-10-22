"""The look's canonical files and the program's answer lines (lab.md, 2.2)."""
import json
from runner import canonical
from runner.records import Refused


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refused(key, "duplicate member")
        result[key] = value
    return result


def _no_number(text):
    raise Refused("$", "floats and nonfinite numbers are forbidden")


def loads(data: bytes, format: str | None) -> dict:
    if type(data) is not bytes:
        raise Refused("$", "expected record bytes")
    try:
        value = json.loads(data.decode("utf-8", "strict"), object_pairs_hook=_pairs,
                           parse_float=_no_number, parse_constant=_no_number)
    except (UnicodeError, ValueError, RecursionError) as error:
        if isinstance(error, Refused):
            raise
        raise Refused("$", "invalid JSON or UTF-8") from error
    if type(value) is not dict:
        raise Refused("$", "a record must be an object")
    encoded = canonical.dumps(value) if format is None else canonical.record_bytes(value)
    if encoded != data:
        raise Refused("$", "noncanonical JSON")
    if format is not None and value.get("format") != format:
        raise Refused("format", "expected " + format)
    return value
