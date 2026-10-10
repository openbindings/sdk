#!/usr/bin/env python3
"""Preparation utility: print content identities only; never approve or run jobs."""
import argparse
import json
from pathlib import Path
from identity import tree
from kernel import sha

parser = argparse.ArgumentParser()
parser.add_argument('paths', type=Path, nargs='+')
args = parser.parse_args()
for path in args.paths:
    path = path.resolve(strict=True)
    if path.is_dir():
        digest, files = tree(path)
        print(json.dumps({'root': str(path), 'treeSha256': digest, 'files': files}))
    else:
        print(json.dumps({'path': str(path), 'sha256': sha(path), 'bytes': path.stat().st_size}))
