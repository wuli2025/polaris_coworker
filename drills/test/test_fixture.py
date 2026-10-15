"""No subprocesses: replace crypto invocation, keep real SQLite/tar verification."""
import base64
import contextlib
import importlib.util
import io
from pathlib import Path
import sqlite3
import re
import struct
import zlib
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('drill_fixture', Path(__file__).parents[1] / 'lib/fixture.py')
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)


class FixtureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='polaris-drill-test-', dir='/tmp')
        self.root = Path(self.temp.name)
        self.data = self.root / 'data'
        self.data.mkdir()
        with sqlite3.connect(self.data / 'collab.db') as db:
            db.execute('CREATE TABLE marker(value TEXT)')
            db.execute("INSERT INTO marker VALUES ('before-backup')")
        (self.data / 'account_authority.key').write_bytes(base64.urlsafe_b64encode(bytes(range(32))).rstrip(b'='))

    def tearDown(self):
        self.temp.cleanup()

    def invoke(self, *args):
        with patch.object(sys, 'argv', ['fixture', *map(str, args)]), contextlib.redirect_stdout(io.StringIO()):
            fixture.main()

    def fake_encrypt(self, source, destination, mode, recipient, timeout):
        self.assertEqual((mode, recipient, timeout), ('age', 'age1fake', 120))
        Path(destination).write_bytes(b'FAKE-AGE\n' + Path(source).read_bytes())

    def fake_decrypt(self, source, destination, env):
        self.assertEqual(env['BACKUP_IDENTITY'], str(self.root / 'identity'))
        body = Path(source).read_bytes()
        self.assertTrue(body.startswith(b'FAKE-AGE\n'))
        Path(destination).write_bytes(body.removeprefix(b'FAKE-AGE\n'))

    def test_backup_restore_validates_and_restores_real_sqlite_and_key(self):
        archive = self.root / 'backup.tar.age'
        original_key = (self.data / 'account_authority.key').read_bytes()
        with patch.object(fixture.snapshot, 'policy', return_value='age'), patch.object(fixture.snapshot, 'encrypt', self.fake_encrypt):
            self.invoke('backup', self.data, archive, 'age1fake')
        with sqlite3.connect(self.data / 'collab.db') as db:
            db.execute("UPDATE marker SET value='after-backup'")
        (self.data / 'account_authority.key').write_bytes(b'changed')
        with patch.object(fixture.snapshot, 'decrypt', self.fake_decrypt):
            self.invoke('restore', archive, self.root / 'restore', self.root / 'identity', self.data)
        with sqlite3.connect(self.data / 'collab.db') as db:
            self.assertEqual(db.execute('SELECT value FROM marker').fetchone()[0], 'before-backup')
        self.assertEqual((self.data / 'account_authority.key').read_bytes(), original_key)
        self.assertFalse((self.root / 'restore').exists())

    def test_corrupt_snapshot_does_not_replace_live_files(self):
        archive = self.root / 'bad.tar.age'
        archive.write_bytes(b'FAKE-AGE\nnot a tar')
        original = (self.data / 'collab.db').read_bytes()
        with patch.object(fixture.snapshot, 'decrypt', self.fake_decrypt):
            with self.assertRaises(Exception):
                self.invoke('restore', archive, self.root / 'restore', self.root / 'identity', self.data)
        self.assertEqual((self.data / 'collab.db').read_bytes(), original)
        self.assertFalse((self.data / 'collab.db.before-restore').exists())

    def test_image_fixture_has_valid_png_chunks_and_compressed_payload(self):
        source = (Path(__file__).parents[1] / 'd1.mjs').read_text()
        encoded = re.search(r"dataBase64: '([^']+)'", source)[1]
        raw = base64.b64decode(encoded)
        self.assertEqual(raw[:8], b'\x89PNG\r\n\x1a\n')
        offset = 8
        compressed = b''
        while offset < len(raw):
            size = struct.unpack('>I', raw[offset:offset + 4])[0]
            chunk = raw[offset + 4:offset + 8 + size]
            crc = struct.unpack('>I', raw[offset + 8 + size:offset + 12 + size])[0]
            self.assertEqual(zlib.crc32(chunk), crc)
            if chunk[:4] == b'IDAT':
                compressed += chunk[4:]
            offset += size + 12
        self.assertTrue(zlib.decompress(compressed))
        self.assertEqual(offset, len(raw))

    def test_fixture_refuses_paths_outside_drill_workspace(self):
        for path in ['/etc/passwd', '/tmp/other.db', self.root / '../outside.db']:
            with self.assertRaises(ValueError):
                fixture.own(path)


if __name__ == '__main__':
    unittest.main()
