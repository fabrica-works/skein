"""Outcome precedence and additive evidence (lab.md, 5.5 and 8)."""
from .canonical import u64
from .records import COST_FIELDS, METER_COST_FIELDS, Refused, positive


def _report_state(report):
    if type(report) is not dict or set(report) != {"done", "retried", "unseen", "surprises"}:
        return "incomplete"
    if type(report["done"]) is not bool or any(type(report[k]) is not list for k in ("retried", "unseen", "surprises")):
        return "incomplete"
    if any(type(s) is not str for s in report["surprises"]):
        return "incomplete"
    for item in report["unseen"]:
        if (type(item) is not dict or set(item) != {"step", "looked_for"}
                or any(type(v) is not str for v in item.values())):
            return "incomplete"
    for item in report["retried"]:
        if (type(item) is not dict or set(item) != {"step", "tries"}
                or type(item["step"]) is not str or type(item["tries"]) is not list):
            return "incomplete"
        for attempt in item["tries"]:
            if (type(attempt) is not dict or set(attempt) != {"tried", "why"}
                    or any(type(v) is not str for v in attempt.values())):
                return "incomplete"
    return "fail" if not report["done"] or report["unseen"] else "pass"


def outcome(failures, errors, undecided, report, *, driver_required=True):
    report_state = _report_state(report) if driver_required else "pass"
    if failures or report_state == "fail":
        return "fail"
    if errors:
        return "error"
    if undecided or report_state == "incomplete":
        return "incomplete"
    return "pass"


def superseded(signatures, held_to, current):
    """Every failure names a class bound now strictly above the run's value."""
    if not signatures or type(held_to) is not dict:
        return False
    for signature in signatures:
        parts = signature.split('/')
        if (len(parts) not in (2, 3) or parts[0] != 'bound' or parts[1] not in COST_FIELDS + METER_COST_FIELDS
                or len(parts) == 3 and not parts[2]):
            return False
        name = parts[1]
        held, now = held_to.get(name), current.get(name)
        if not positive(held) or not positive(now) or now <= held:
            return False
    return True


def superseded_by_evidence(evidence, bounds):
    """Within one key, later judged runs prove raised bounds of the same class."""
    result = set()
    for index, line in enumerate(evidence):
        if line['outcome'] != 'fail':
            continue
        held = bounds[line['run']]
        for later in evidence[index + 1:]:
            if later['outcome'] not in ('pass', 'fail'):
                continue
            raised = bounds[later['run']]
            if held['name'] == raised['name'] and superseded(line['signatures'], held, raised):
                result.add(line['run'])
                break
    return frozenset(result)


def _evidence(evidence, superseded=frozenset()):
    if type(evidence) not in (list, tuple):
        raise Refused("evidence", "expected evidence records in append order")
    seeds = []
    judged = []
    for item in evidence:
        if type(item) is not dict or "seed" not in item or item.get("outcome") not in ("pass", "fail", "error", "incomplete"):
            raise Refused("evidence", "expected seed and outcome")
        seed = item["seed"]
        u64(seed)
        if seed not in seeds:
            if len(seeds) == 3:
                raise Refused("seed", "a fourth seed is forbidden")
            seeds.append(seed)
        if item["outcome"] in ("pass", "fail") and item.get('run') not in superseded:
            judged.append(item)
    return seeds, judged


def seed_admitted(evidence, seed):
    u64(seed)
    seeds, _ = _evidence(evidence)
    return seed in seeds or len(seeds) < 3


def verdict(evidence, superseded=frozenset()):
    seeds, judged = _evidence(evidence, superseded)
    if len(seeds) != 3 or any(not any(r["seed"] == s for r in judged) for s in seeds):
        return "not enough evidence"
    outcomes = {item["outcome"] for item in judged}
    return next(iter(outcomes)) if len(outcomes) == 1 else "flaky"


def next_seed(evidence, superseded=frozenset()):
    """A pending seed requires an explicit run; judge never retries it."""
    seeds, judged = _evidence(evidence, superseded)
    pending = [s for s in seeds if not any(r['seed'] == s for r in judged)]
    if pending:
        return dict(action='blocked', seed=pending[0])
    if len(seeds) == 3:
        return dict(action='done', verdict=verdict(evidence, superseded))
    return dict(action='draw', excluded=seeds)


def verdict_record(key, evidence, superseded=frozenset()):
    seeds, judged = _evidence(evidence, superseded)
    return dict(format='lab-verdict/1', key=key, policy='three-seeds', verdict=verdict(evidence, superseded),
                seeds=[dict(seed=s, state=('judged' if any(r['seed'] == s for r in judged) else 'not judged'),
                            judged=[r['run'] for r in judged if r['seed'] == s],
                            not_judged=[r['run'] for r in evidence if r['seed'] == s and r not in judged])
                       for s in seeds])
