#!/usr/bin/env python3
"""Stopped-authority snapshots: normalize SQLite WAL, strictly validate before restore."""
import base64
import hashlib
import json
import os
import sqlite3
import sys
import tarfile
import tempfile
from datetime import datetime, timezone
from pathlib import Path

PAYLOAD = ('collab.db', 'account_authority.key')
MEMBERS = {*PAYLOAD, 'manifest.json'}


def key_check(data):
    raw = data.strip()
    seed = base64.b64decode(raw + b'=' * (-len(raw) % 4), altchars=b'-_', validate=True)
    if len(seed) != 32:
        raise ValueError('签名私钥必须是 32 字节种子')


def check_db(path):
    with sqlite3.connect(path.as_uri() + '?mode=ro', uri=True) as db:
        if db.execute('PRAGMA integrity_check').fetchall() != [('ok',)]:
            raise ValueError('SQLite 完整性检查失败')
        if not db.execute("SELECT 1 FROM sqlite_master WHERE type='table'").fetchone():
            raise ValueError('拒绝备份/恢复空数据库')


def pack(source, destination, image):
    source, destination = Path(source).resolve(), Path(destination).resolve()
    for name in PAYLOAD:
        if (source / name).is_symlink() or not (source / name).is_file():
            raise ValueError(f'缺失或不安全的备份文件: {name}')
    key_check((source / PAYLOAD[1]).read_bytes())
    with tempfile.TemporaryDirectory(dir=destination, prefix='.normalize-') as folder:
        normalized = Path(folder)
        # source 是停止后复制的目录；普通读写打开以正确恢复 WAL，不使用 immutable=1。
        with sqlite3.connect(source / 'collab.db') as src:
            with sqlite3.connect(normalized / 'collab.db') as dst:
                src.backup(dst)
        check_db(normalized / 'collab.db')
        (normalized / PAYLOAD[1]).write_bytes((source / PAYLOAD[1]).read_bytes())
        manifest = {'format': 1, 'authorityImage': image, 'sha256': {}}
        for name in PAYLOAD:
            manifest['sha256'][name] = hashlib.sha256((normalized / name).read_bytes()).hexdigest()
        (normalized / 'manifest.json').write_text(json.dumps(manifest, indent=2))
        stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
        target = destination / ('authority-' + stamp + '.tar')
        partial = target.with_suffix('.partial')
        try:
            with tarfile.open(partial, 'x') as archive:
                for name in sorted(MEMBERS):
                    info = archive.gettarinfo(str(normalized / name), arcname=name)
                    info.uid = info.gid = 1000
                    info.uname = info.gname = ''
                    info.mode = 0o600
                    with (normalized / name).open('rb') as body:
                        archive.addfile(info, body)
            verify(partial)
            partial.rename(target)
        except Exception:
            partial.unlink(missing_ok=True)
            raise
        print(target)


def verify(path):
    with tarfile.open(path, 'r:') as archive:
        members = archive.getmembers()
        if len(members) != len(MEMBERS) or {m.name for m in members} != MEMBERS:
            raise ValueError('备份成员不匹配，拒绝路径穿越/重复文件/额外文件')
        if any(not m.isfile() or m.issym() or m.islnk() for m in members):
            raise ValueError('备份只允许普通文件')
        meta = archive.extractfile('manifest.json')
        if meta is None or archive.getmember('manifest.json').size > 16384:
            raise ValueError('非法 manifest')
        manifest = json.load(meta)
        if manifest.get('format') != 1 or set(manifest.get('sha256', {})) != set(PAYLOAD):
            raise ValueError('不支持的备份格式')
        with tempfile.TemporaryDirectory(prefix='polaris-verify-') as folder:
            for name in PAYLOAD:
                digest = hashlib.sha256()
                target = Path(folder) / name
                with target.open('wb') as output, archive.extractfile(name) as body:
                    while chunk := body.read(1024 * 1024):
                        digest.update(chunk)
                        output.write(chunk)
                if digest.hexdigest() != manifest['sha256'][name]:
                    raise ValueError(f'校验和不匹配: {name}')
            key_check((Path(folder) / PAYLOAD[1]).read_bytes())
            check_db(Path(folder) / 'collab.db')
    return manifest


if __name__ == '__main__':
    os.umask(0o077)
    try:
        if sys.argv[1] == 'pack':
            pack(*sys.argv[2:])
        elif sys.argv[1] == 'verify':
            verify(sys.argv[2])
            print('备份格式、校验和、SQLite、私钥种子格式：PASS')
        else:
            raise ValueError('未知动作')
    except (OSError, ValueError, sqlite3.Error, tarfile.TarError) as error:
        sys.exit(str(error))
