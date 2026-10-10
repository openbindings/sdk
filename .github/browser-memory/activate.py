#!/usr/bin/env python3
"""PROPOSAL: restore an explicitly reviewed artifact; later invoke unchanged harness.

stage validates/restores only. run requires a prior uploaded activation marker.
Never creates its own approval, rebuilds a package or changes a frozen binding.
"""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import sys
import urllib.parse
import zipfile
from common import CANONICAL, bounded, check_preparer, check_seed, require, safe_extract, save, sha
from gate import activation


def gh_json(path):
    return json.loads(subprocess.run(['gh', 'api', '--method', 'GET', path], capture_output=True, text=True, check=True, timeout=60).stdout)


def unzip_regular(archive, destination):
    with zipfile.ZipFile(archive) as source:
        members, seen, total = source.infolist(), set(), 0
        require(len(members) <= 1000, 'artifact ZIP member count exceeded')
        for member in members:
            name = PurePosixPath(member.filename)
            require(not name.is_absolute() and '..' not in name.parts and str(name) == member.filename.rstrip('/'), 'unsafe artifact ZIP path')
            require(str(name) not in seen, 'duplicate ZIP member'); seen.add(str(name))
            mode = member.external_attr >> 16
            require(not stat.S_ISLNK(mode) and (not stat.S_IFMT(mode) or stat.S_ISREG(mode) or stat.S_ISDIR(mode)), 'special artifact ZIP member')
            total += member.file_size
            require(total <= 3 * 1024**3, 'artifact ZIP size cap')
        Path(destination).mkdir()
        source.extractall(destination)


def review_bundle(request, approval_path, root):
    require(sha(approval_path) == request['ownerReviewSha256'], 'approval bytes differ from activation')
    approval = json.loads(Path(approval_path).read_text())
    require(approval['status'] == 'approved' and approval['independentOfHarnessImplementation'] is True,
            'final independent binding approval required')
    require(isinstance(approval['reviewer'], str) and approval['reviewer'].strip(), 'reviewer identity missing')
    require(approval['sourceFreezeSha256'] == request['sourceFreezeSha256'] and approval['bindingsSha256'] == request['bindingsSha256'], 'approval binds different source/artifacts')
    require(approval['acceptsKernelChargeScope'] is True and approval['acceptsAdmissionAndCleanup'] is True, 'required review scope absent')
    check_seed(root)
    proposed = root / 'BINDINGS.proposed.json'
    require(sha(proposed) == request['bindingsSha256'], 'prepared binding differs')
    bindings = json.loads(proposed.read_text())
    provenance = json.loads((root / 'PREPARATION.json').read_text())
    require(provenance['preparerFreezeSha256'] == request['preparerFreezeSha256'], 'prepared bundle used different preparation sources')
    require(provenance['runner']['GITHUB_RUN_ID'] == str(request['preparationRunId']) and
            provenance['runner']['GITHUB_RUN_ATTEMPT'] == str(request['preparationRunAttempt']), 'prepared receipt run/attempt differs')
    for name in ('baseline', 'candidate'):
        require(bindings['artifacts'][name]['sourceCommit'] == request[name + 'Commit'], 'source commit differs')
        require(bindings['artifacts'][name]['archive']['sha256'] == request[name + 'ArchiveSha256'], 'explicit archive binding differs')
    require(os.environ.get('ImageVersion') == bindings['host']['imageVersion'], 'fresh runner image differs; new preparation/review required')
    return bindings


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', choices=('stage', 'run'))
    parser.add_argument('--activation', type=Path, required=True)
    parser.add_argument('--approval', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    request = activation(json.loads(args.activation.read_text()))
    require(check_preparer() == request['preparerFreezeSha256'], 'activation covers different preparation scripts')
    require(os.environ.get('GITHUB_RUN_ATTEMPT') == '1', 'no workflow reruns')
    prefix = '/repos/openbindings/sdk/actions/'
    marker_name = 'browser-memory-activation-' + request['activationId']
    args.evidence.mkdir(parents=True, exist_ok=True)
    try:
        if args.phase == 'stage':
            markers = gh_json(prefix + 'artifacts?name=' + urllib.parse.quote(marker_name))['artifacts']
            require(not markers, 'activation already consumed; preserve prior outcome and seek new prospective review')
            metadata = gh_json(prefix + 'artifacts/' + str(request['artifactId']))
            require(not metadata['expired'] and metadata['name'] == request['artifactName'] and metadata['workflow_run']['id'] == request['preparationRunId'], 'artifact ID/run/name differs or expired')
            save(args.evidence / 'INPUT-ARTIFACT.json', metadata)
            downloaded = args.evidence / 'input-artifact.zip'
            with downloaded.open('xb') as out:
                subprocess.run(['gh', 'api', '--method', 'GET', prefix + 'artifacts/' + str(request['artifactId']) + '/zip'], stdout=out, check=True, timeout=180)
            unzip_regular(downloaded, args.evidence / 'download')
            bundle = args.evidence / 'download/prepared-inputs.tar.gz'
            require(sha(bundle) == request['bundleSha256'], 'prepared bundle digest differs')
            require(not CANONICAL.exists(), 'canonical destination already exists; refusing reuse')
            # Parent /opt is root-owned. Extract into a fresh runner-owned temporary
            # directory, then move the verified directory atomically to its fixed path.
            extracted = args.evidence / 'extracted'
            safe_extract(bundle, extracted)
            subprocess.run(['sudo', '-n', 'mv', '--no-clobber', str(extracted), str(CANONICAL)], check=True, timeout=30)
            require(not extracted.exists() and CANONICAL.is_dir(), 'canonical restore was not installed')
            bindings = review_bundle(request, args.approval, CANONICAL)
            # Preserve exactly the reviewed bytes; do not serialize/rebind them.
            (CANONICAL / 'BINDINGS.json').write_bytes((CANONICAL / 'BINDINGS.proposed.json').read_bytes())
            (CANONICAL / 'OWNER-REVIEW.json').write_bytes(args.approval.read_bytes())
            receipt = {'activationId': request['activationId'], 'runId': os.environ['GITHUB_RUN_ID'],
                       'artifactId': request['artifactId'], 'bundleSha256': request['bundleSha256'],
                       'bindingsSha256': request['bindingsSha256'], 'ownerReviewSha256': request['ownerReviewSha256'],
                       'sdkAttemptConsumed': False, 'scope': 'Restored reviewed inputs only; marker must be uploaded before run'}
            save(args.evidence / 'ACTIVATION-CLAIM.json', receipt)
            return 0
        review_bundle(request, args.approval, CANONICAL)
        claim = json.loads((args.evidence / 'ACTIVATION-CLAIM.json').read_text())
        require(claim['runId'] == os.environ['GITHUB_RUN_ID'] and claim['activationId'] == request['activationId'], 'stage receipt belongs to another run')
        markers = gh_json(prefix + 'artifacts?name=' + urllib.parse.quote(marker_name))['artifacts']
        require(len(markers) == 1 and str(markers[0]['workflow_run']['id']) == os.environ['GITHUB_RUN_ID'], 'exact current-run activation marker not visible; fail closed')
        require(sha(CANONICAL / 'BINDINGS.json') == request['bindingsSha256'] and sha(CANONICAL / 'OWNER-REVIEW.json') == request['ownerReviewSha256'], 'staged binding/approval changed')
        with (args.evidence / 'install-deps.stdout').open('wb') as out, (args.evidence / 'install-deps.stderr').open('wb') as err:
            completed = bounded([str(CANONICAL / 'node/bin/node'), str(CANONICAL / 'playwright/cli.js'), 'install-deps', 'chromium', 'webkit'], stdout=out, stderr=err, timeout=900, privileged_children=True)
            require(completed.returncode == 0, 'browser dependencies failed; preserve logs, no retry')
        command = ['sudo', '-n', '--preserve-env=ImageOS,ImageVersion,RUNNER_OS,RUNNER_ARCH,GITHUB_RUN_ID,GITHUB_RUN_ATTEMPT',
                   '/usr/bin/python3', str(CANONICAL / 'harness/campaign.py'), '--bindings', str(CANONICAL / 'BINDINGS.json'),
                   '--review', str(CANONICAL / 'OWNER-REVIEW.json'), '--uid', str(os.getuid()), '--gid', str(os.getgid()),
                   '--output', str(args.evidence / 'campaign')]
        save(args.evidence / 'CAMPAIGN-COMMAND.json', {'argv': command, 'activationId': request['activationId']})
        with (args.evidence / 'campaign.stdout').open('wb') as out, (args.evidence / 'campaign.stderr').open('wb') as err:
            completed = subprocess.run(command, stdout=out, stderr=err)
        return completed.returncode
    except Exception as error:
        save(args.evidence / (args.phase + '-FAILURE.json'), {'error': repr(error), 'phase': args.phase, 'activationId': request['activationId'], 'noAutomaticRetry': True})
        return 1


if __name__ == '__main__': sys.exit(main())
