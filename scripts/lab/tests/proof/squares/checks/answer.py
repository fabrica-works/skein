#!/usr/bin/python3
"""Judge the service's observations without executing the submitted program."""
import json
from pathlib import Path
import sys


def judge(observations):
    if observations is None:
        return 2, [{'undecided': 'observations unreadable'}]
    calls = observations['calls']
    failures = []
    if not any(c['verb'] == 'program' and c['bytes'] > 0 for c in calls):
        failures.append(dict(fail='program/missing', summary='No nonempty program received.'))
    answers = [c for c in calls if c['verb'] == 'answer']
    if not answers:
        failures.append(dict(fail='answer/missing', summary='No answer received.'))
    if not answers or answers[-1]['verdict'] != 'accepted':
        failures.append(dict(fail='answer/wrong', summary='The last answer is not accepted.'))
    return (1 if failures else 0), failures


def main(args):
    try:
        observations = json.loads(Path(args[0]).read_text(encoding='utf-8'))
        status, lines = judge(observations)
    except (OSError, UnicodeError, ValueError, KeyError, TypeError, IndexError):
        status, lines = judge(None)
    for line in lines:
        print(json.dumps(line))
    return status


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
