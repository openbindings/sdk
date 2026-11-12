#!/usr/bin/env python3
"""PR event selection. Templates/mismatched actors/reruns cannot execute a phase."""
import argparse
import json
import os
from pathlib import Path
import re
from common import HARNESS_FREEZE, digest, require, save, validate_request


def activation(request):
    require(request['schemaVersion'] == 1 and request['mode'] == 'campaign', 'wrong activation phase')
    require(request['sdkRepository'] == 'openbindings/sdk', 'repository scope changed')
    require(isinstance(request['ownerLogin'], str) and request['ownerLogin'], 'explicit owner required')
    require(re.fullmatch('[0-9a-f]{32}', request['activationId'] or ''), 'fresh activation ID required')
    for key in ('preparationRunId', 'preparationRunAttempt', 'artifactId'):
        require(type(request[key]) is int and request[key] > 0, 'explicit artifact run/attempt/id required')
    require(request['artifactName'] == f"browser-memory-inputs-{request['preparationRunId']}-{request['preparationRunAttempt']}", 'artifact name differs')
    for key in ('bundleSha256', 'bindingsSha256', 'ownerReviewSha256', 'candidateArchiveSha256', 'baselineArchiveSha256', 'preparerFreezeSha256'):
        digest(request[key])
    digest(request['candidateCommit'], 40); digest(request['baselineCommit'], 40)
    require(request['sourceFreezeSha256'] == HARNESS_FREEZE, 'accepted harness differs')
    return request


def select(request, event, repository, run_attempt):
    require(run_attempt == '1', 'workflow rerun denied; use a newly reviewed prospective activation')
    require(event['pull_request']['head']['repo']['full_name'] == repository == 'openbindings/sdk', 'same-repository SDK PR required')
    require(event['sender']['login'] == request['ownerLogin'], 'only explicit root actor may activate preparation/campaign')
    require(event['pull_request']['state'] == 'open', 'open PR required')
    if request['mode'] == 'prepare':
        validate_request(request)
        require(event['action'] in ('opened', 'synchronize', 'reopened'), 'preparation requires a new/reopened/updated PR')
        return 'prepare'
    activation(request)
    if event['action'] != 'labeled': return 'none'
    require(event['label']['name'] == 'sdk-browser-memory-campaign-approved', 'explicit campaign activation label required')
    return 'campaign'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    args.evidence.mkdir(parents=True, exist_ok=False)
    try:
        request = json.loads(args.request.read_text())
        event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
        phase = select(request, event, os.environ['GITHUB_REPOSITORY'], os.environ['GITHUB_RUN_ATTEMPT'])
        save(args.evidence / 'GATE.json', {'status': 'selected', 'phase': phase, 'headCommit': event['pull_request']['head']['sha'], 'actor': event['sender']['login']})
        with open(os.environ['GITHUB_OUTPUT'], 'a') as out:
            out.write('phase=' + phase + '\n')
            if phase == 'campaign': out.write('activation=' + request['activationId'] + '\n')
    except Exception as error:
        save(args.evidence / 'GATE.json', {'status': 'rejected', 'error': repr(error)})
        raise
