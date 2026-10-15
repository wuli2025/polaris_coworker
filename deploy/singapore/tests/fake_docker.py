#!/usr/bin/env python3
"""Offline Docker double; only simulates commands exercised by maintenance tests."""
import json
import os
import shutil
import sys
import tarfile
from pathlib import Path

root = Path(os.environ['FAKE_DOCKER_ROOT'])
state_file = root / 'state.json'
state = json.loads(state_file.read_text())
args = sys.argv[1:]
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps(args) + '\n')
if args[0] == 'compose':
    args = args[1:]
    while args and args[0] in ('--project-directory', '--env-file', '-f'):
        args = args[2:]
    action = args[0]
    if os.environ.get('FAKE_FAIL') == action:
        sys.exit(17)
    if action in ('version', 'config', 'pull', 'create'):
        pass
    elif action == 'ps':
        print('fake-authority')
    elif action == 'stop':
        state['running'] = False
    elif action in ('start', 'up'):
        state['running'] = True
    elif action == 'run':
        script = args[-1]
        if 'test -s' in script:
            sys.exit(0 if (root / 'data/collab.db').exists() else 1)
        elif 'tar -xf' in script:
            if os.environ.get('FAKE_FAIL') == 'extract':
                sys.exit(18)
            with tarfile.open(fileobj=sys.stdin.buffer, mode='r|') as archive:
                for member in archive:
                    if member.name in ('collab.db', 'account_authority.key'):
                        (root / 'data' / member.name).write_bytes(archive.extractfile(member).read())
        elif '已有数据库' in script:
            if (root / 'data/collab.db').exists() and not (root / 'data/account_authority.key').exists():
                sys.exit(20)
        else:
            raise RuntimeError('unhandled run')
    else:
        raise RuntimeError(f'unhandled compose {args}')
elif args[0] == 'inspect':
    if args[2] == '{{.State.Running}}':
        print(str(state['running']).lower())
    elif args[2] == '{{.Config.Image}}':
        print('example.invalid/authority:previous@sha256:' + '1' * 64)
    else:
        raise RuntimeError('unhandled inspect')
elif args[0] == 'cp':
    if os.environ.get('FAKE_FAIL') == 'cp':
        sys.exit(19)
    shutil.copytree(root / 'data', args[2], dirs_exist_ok=True)
else:
    raise RuntimeError(f'unhandled docker {args}')
state_file.write_text(json.dumps(state))
