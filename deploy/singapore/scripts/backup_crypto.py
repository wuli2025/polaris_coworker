#!/usr/bin/env python3
"""External encryption tools and private plaintext workspaces; stdlib only."""
import os
import shutil
import stat
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path


def clean(folder):
    """Overwrite/fsync/unlink our private temporary files; never follow symlinks.

    CoW/SSD/journal history still requires encrypted storage or tmpfs (see README).
    """
    folder = Path(folder)
    if not folder.exists():
        return
    errors = []
    for root, dirs, files in os.walk(folder, topdown=False, followlinks=False):
        for name in files:
            path = Path(root) / name
            try:
                if stat.S_ISREG(path.lstat().st_mode):
                    fd = os.open(path, os.O_WRONLY | os.O_NOFOLLOW)
                    with os.fdopen(fd, 'wb') as output:
                        remaining = os.fstat(output.fileno()).st_size
                        zeros = b'\0' * (1024 * 1024)
                        while remaining:
                            count = output.write(zeros[:min(remaining, len(zeros))])
                            remaining -= count
                        output.flush()
                        os.fsync(output.fileno())
            except OSError as error:
                errors.append(error)
            finally:
                path.unlink(missing_ok=True)
        for name in dirs:
            path = Path(root) / name
            if path.is_symlink():
                path.unlink()
            else:
                path.rmdir()
    folder.rmdir()
    if errors:
        raise OSError(f'明文覆盖清理失败（已尝试删除）：{errors[0]}')


@contextmanager
def workspace():
    folder = Path(tempfile.mkdtemp(prefix='polaris-sensitive-'))
    try:
        yield folder
    finally:
        clean(folder)


def tool(name):
    if not shutil.which(name):
        raise ValueError(f'BLOCKED: 服务器缺少 {name}，请按 README 安装；不会退回明文')
    return name


def policy(env):
    mode, recipient = env['BACKUP_ENCRYPT'], env['BACKUP_RECIPIENT']
    if mode and recipient:
        tool(mode)
        return mode
    if env['BACKUP_PLAINTEXT_ACK'] != '1':
        raise ValueError('BLOCKED: 配置 BACKUP_ENCRYPT=age|gpg 与 BACKUP_RECIPIENT；'
                         '仅确认备份目录为加密介质后才能设 BACKUP_PLAINTEXT_ACK=1')
    print('!!! WARNING: 明文备份含签名私钥；BACKUP_PLAINTEXT_ACK=1 已确认加密介质 !!!',
          file=sys.stderr)
    return ''


def encrypt(source, destination, mode, recipient):
    if mode == 'age':
        command = [tool('age'), '--encrypt', '--recipient', recipient]
    elif mode == 'gpg':
        command = [tool('gpg'), '--no-options', '--batch', '--no-tty', '--output', '-',
                   '--trust-model', 'always',
                   '--encrypt', '--recipient', recipient]
    else:
        raise ValueError('未知加密格式')
    # Output to a private exclusive file; never expose a successful archive on failure.
    with Path(source).open('rb') as body, Path(destination).open('xb') as output:
        subprocess.run(command, stdin=body, stdout=output, check=True)


def decrypt(source, destination, env):
    suffix = Path(source).suffix
    if suffix == '.age':
        identity = env['BACKUP_IDENTITY']
        if not identity or not Path(identity).is_file():
            raise ValueError('BLOCKED: age 恢复需要 BACKUP_IDENTITY 指向私钥文件')
        command = [tool('age'), '--decrypt', '--identity', identity]
    elif suffix == '.gpg':
        command = [tool('gpg'), '--no-options', '--batch', '--no-tty', '--output', '-', '--decrypt']
    else:
        raise ValueError('恢复仅接受 .tar / .tar.age / .tar.gpg')
    with Path(source).open('rb') as body, Path(destination).open('xb') as output:
        subprocess.run(command, stdin=body, stdout=output, check=True)
