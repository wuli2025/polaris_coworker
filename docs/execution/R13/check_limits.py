#!/usr/bin/env python3
"""Check only R13-owned additions and the tracked diff; do not scan pre-existing user additions."""
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True)


def paths():
    changed = git('diff', '--name-only').splitlines()
    added = git('ls-files', '--others', '--exclude-standard',
                'src-tauri/crates/polaris-collab/src/collab',
                'src-tauri/src/hosting', 'deploy/singapore/tests/test_r13.py',
                'docs/execution/R13').splitlines()
    return sorted(set(changed + added)), set(added)


def main():
    files, added = paths()
    forbidden = re.compile(r'\.(unwrap|expect)\s*\(')
    for name in files:
        lines = (ROOT / name).read_text().splitlines()
        assert len(lines) <= 500, (name, len(lines))
        assert all(len(line) <= 500 for line in lines), name
        if name in added and name.endswith('.rs'):
            assert not any(forbidden.search(line) for line in lines), name
    diff = git('diff', '--unified=0', '--', '*.rs')
    assert not any(forbidden.search(line) for line in diff.splitlines() if line.startswith('+'))
    for name in ('check-desktop.log', 'check-server.log', 'check-server-net.log',
                 'check-collab.log', 'collab-tests.log'):
        assert 'warning:' not in (Path(__file__).parent / name).read_text(), name
    print(f'{len(files)} R13 changed/new files: <=500 lines, <=500 characters/line; '
          'no new Rust unwrap()/expect(); build/test logs contain no warnings')


if __name__ == '__main__':
    main()
