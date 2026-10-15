#!/usr/bin/env python3
"""TEST DOUBLE ONLY: reversible transport, NOT encryption. Never used by production."""
import base64
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]
with (Path(os.environ['FAKE_DOCKER_ROOT']) / 'crypto.jsonl').open('a') as log:
    log.write(json.dumps([Path(sys.argv[0]).name, *args]) + '\n')
if os.environ.get('FAKE_CRYPTO_FAIL') == '1':
    sys.stdout.buffer.write(b'partial failure')
    sys.exit(29)
body = sys.stdin.buffer.read()
if '--encrypt' in args:
    sys.stdout.buffer.write(b'TEST-DOUBLE:' + base64.b64encode(body))
elif '--decrypt' in args:
    if not body.startswith(b'TEST-DOUBLE:'):
        sys.exit(30)
    sys.stdout.buffer.write(base64.b64decode(body.removeprefix(b'TEST-DOUBLE:'), validate=True))
else:
    sys.exit(31)
