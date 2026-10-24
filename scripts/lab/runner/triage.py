"""Triage input, answers and additive finding decisions (lab.md, 9.2)."""
import json
from . import canonical, findings
from .records import CLASSES, Refused, shape, signature_value, text_value


def input_value(run, history, evidence, report=None, carried=None):
    if run['outcome'] != 'fail':
        raise Refused('triage.run', 'a judged failing run is required')
    candidates = {findings.occurrence(values[0]): identity for identity, values in history.items()}
    failures = [f for f in run['failures'] if f['kind'] == 'use'
                or findings.occurrence(findings.reference(run, f)) in candidates]
    if not failures:
        raise Refused('triage.failures', 'no use failures or candidates created by this run')
    return dict(run=run['run'], key=run['key'], seed=run['seed'], failures=failures,
                report=report, findings=findings.index(history, evidence, carried)['findings'], classes=list(CLASSES))


def output_value(text, supplied):
    try:
        value = json.loads(text, object_pairs_hook=canonical._pairs, parse_constant=canonical._no_number)
    except (ValueError, TypeError) as error:
        raise Refused('triage.output', 'final agentMessage is not triage JSON') from error
    problems = []
    obj = shape(value, ('failures',), (), 'triage.output', problems)
    answers = obj.get('failures')
    if type(answers) is not list:
        raise Refused('triage.output.failures', 'expected a list')
    failures = {f['id']: f for f in supplied['failures']}
    identities = {f['id'] for f in supplied['findings']}
    seen = set()
    for n, answer in enumerate(answers):
        member = f'triage.output.failures.{n}'
        row = shape(answer, ('id', 'match', 'signature', 'class', 'title', 'reason'), (), member, problems)
        identity, match = row.get('id'), row.get('match')
        if type(identity) is not str or identity not in failures or identity in seen:
            raise Refused(member + '.id', 'expected each input failure exactly once')
        seen.add(identity)
        if match is not None and (type(match) is not str or match not in identities):
            raise Refused(member + '.match', 'expected an input finding id or null')
        use = failures[identity]['kind'] == 'use'
        signature = row.get('signature')
        if use and match is None:
            if not signature_value(signature) or not signature.startswith('use/') or len(signature) <= 4:
                raise Refused(member + '.signature', 'expected use/ followed by printable ASCII without spaces')
        elif signature is not None:
            raise Refused(member + '.signature', 'signature must be null unless proposing a use failure')
        finding_class = row.get('class')
        if finding_class not in CLASSES and not (finding_class is None and match is not None and use):
            raise Refused(member + '.class', 'a proposed class is required unless matching a use failure')
        for field in ('title', 'reason'):
            if not text_value(row.get(field)):
                raise Refused(member + '.' + field, 'expected nonempty text')
    if seen != set(failures):
        raise Refused('triage.output.failures', 'expected each input failure exactly once')
    if problems:
        raise Refused(problems[0].member, problems[0].why)
    canonical.dumps(value)
    return value


def additions(supplied, output, history, evidence, t, carried=None):
    """Validate the whole fold before the host appends its first event."""
    output = output_value(canonical.dumps(output), supplied)
    histories = {identity: list(values) for identity, values in history.items()}
    run = next(r for r in evidence[supplied['key']] if r['run'] == supplied['run'])
    failures = {f['id']: f for f in supplied['failures']}
    result = []
    for answer in output['failures']:
        failure = failures[answer['id']]
        fields = findings.reference(run, failure)
        identity = answer['match']
        if failure['kind'] == 'use' and identity is not None:
            state = findings.fold(histories[identity], evidence, carried)
            if findings.occurrence(fields) in {findings.occurrence(v) for v in state['occurrences']}:
                continue
            values = [findings.event('occurred', t, 'triager', **fields)]
        else:
            reason = answer['reason']
            if failure['kind'] == 'use':
                identity = f'F-{len(histories) + 1:04d}'
                values = [findings.event('candidate', t, 'triager', signature=answer['signature'],
                    kind='use', summary=failure['summary'], **fields)]
            else:
                identity = next(i for i, values in histories.items()
                                if findings.occurrence(values[0]) == findings.occurrence(fields))
                values = []
                if answer['match'] is not None:
                    reason += '\nSuggested match: ' + answer['match']
            values.append(findings.event('proposed', t, 'triager', title=answer['title'],
                                          reason=reason, **{'class': answer['class']}))
        histories.setdefault(identity, []).extend(values)
        findings.fold(histories[identity], evidence, carried)
        result.extend((identity, value) for value in values)
    findings.index(histories, evidence, carried)
    return result


def token_total(observed):
    """Input includes cached tokens; no unknown component is inferred to be zero."""
    turns = [turn for turn in observed['turns'] if turn['completed_ms'] is not None]
    values = [turn['tokens'][field] for turn in turns for field in ('input', 'output')]
    return sum(values) if turns and all(v is not None for v in values) else None
