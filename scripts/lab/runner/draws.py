"""Seeded draws and serial ready-set scheduling (lab.md, 5.4)."""
import hashlib
from .canonical import u64, u64_text
from .records import Refused


def draw(seed, label):
    u64(seed)
    if type(label) is not str or not label or "\x00" in label:
        raise Refused("label", "expected a nonempty label without NUL")
    data = b"lab-seed\0" + seed.encode("ascii") + b"\0" + label.encode("utf-8", "strict")
    return u64_text(int.from_bytes(hashlib.sha256(data).digest()[:8], "big"))


def start_order(seed, after):
    u64(seed)
    if type(after) is not dict or any(type(name) is not str for name in after):
        raise Refused("after", "expected named dependencies")
    for name, dependencies in after.items():
        if (type(dependencies) is not list or any(type(d) is not str or d not in after for d in dependencies)
                or len(set(dependencies)) != len(dependencies)):
            raise Refused(name + ".after", "invalid dependencies")
    order = []
    while len(order) < len(after):
        ready = sorted(n for n, deps in after.items() if n not in order and all(d in order for d in deps))
        if not ready:
            raise Refused("after", "dependency cycle")
        order.append(ready[u64(draw(seed, f"start-order.{len(order)}")) % len(ready)])
    return order


def fault_n(seed, index, lo, hi):
    if type(index) is not int or index < 0 or type(lo) is not int or type(hi) is not int or not 1 <= lo <= hi:
        raise Refused("fault", "expected nonnegative index and positive inclusive range")
    return lo + u64(draw(seed, f"fault.{index}")) % (hi - lo + 1)
