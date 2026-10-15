"""Only generated drill paths; no user DB discovery, no credentials on stdout."""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import sys
import tarfile

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / 'deploy/singapore/scripts'))
import snapshot  # noqa: E402


def own(path):
    value = Path(path).resolve()
    if not str(value).startswith('/tmp/polaris-drill-'):
        raise ValueError('Fixture path must belong to a temporary drill')
    return value


def main():
    operation, *args = sys.argv[1:]
    if operation == 'owner':
        with sqlite3.connect(own(args[0])) as db:
            changed = db.execute("UPDATE users SET role='owner' WHERE username='drill-user'").rowcount
            for i in range(4):
                email = f'registered-{i}@example.invalid'
                row = db.execute("SELECT * FROM users WHERE username='drill-user'").fetchone()
                columns = [r[1] for r in db.execute('PRAGMA table_info(users)')]
                values = dict(zip(columns, row))
                values.pop('id')
                values.update(username=f'drill-mail-{i}', email=email, uid=f'drill-mail-uid-{i}')
                cols = ','.join(values)
                placeholders = ','.join('?' for _ in values)
                db.execute(f'INSERT INTO users ({cols}) VALUES ({placeholders})', list(values.values()))
            if changed != 1:
                raise ValueError('Seeded account missing')
    elif operation == 'backup':
        source, archive, recipient = args
        archive = own(archive)
        before = set(archive.parent.glob('authority-*.tar.age'))
        snapshot.pack(str(own(source)), str(archive.parent), 'local-drill', {
            'BACKUP_ENCRYPT': 'age', 'BACKUP_RECIPIENT': recipient,
            'BACKUP_PLAINTEXT_ACK': '0', 'BACKUP_CRYPTO_TIMEOUT_SECS': '120',
        })
        generated = set(archive.parent.glob('authority-*.tar.age')) - before
        if len(generated) != 1:
            raise ValueError('Expected one encrypted snapshot')
        generated.pop().rename(archive)
    elif operation == 'restore':
        archive, work, identity, destination = map(own, args)
        work.mkdir(parents=True, exist_ok=True)
        plain = snapshot.unpack(str(archive), str(work), {
            'BACKUP_ENCRYPT': 'age', 'BACKUP_IDENTITY': str(identity),
            'BACKUP_CRYPTO_TIMEOUT_SECS': '120',
        })
        for name in snapshot.PAYLOAD:
            (destination / name).rename(destination / (name + '.before-restore'))
        # snapshot.unpack verifies members, hashes, key and SQLite before mutation.
        with tarfile.open(plain) as tar:
            for name in snapshot.PAYLOAD:
                with tar.extractfile(name) as src, (destination / name).open('wb') as dst:
                    shutil.copyfileobj(src, dst)
                os.chmod(destination / name, 0o600)
        for name in ['collab.db-wal', 'collab.db-shm']:
            (destination / name).unlink(missing_ok=True)
        snapshot.clean(work)
    else:
        raise ValueError('Unknown fixture operation')


if __name__ == '__main__':
    os.umask(0o077)
    main()
