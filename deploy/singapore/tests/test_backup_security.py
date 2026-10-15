#!/usr/bin/env python3
import json
import os
import shutil
import sqlite3
import unittest
from contextlib import closing

import test_behavior as support


class BackupSecurityTests(support.MaintenanceFixture, unittest.TestCase):
    def crypto(self, mode):
        shutil.copy(support.ROOT / 'tests/fake_crypto.py', self.bin / mode)
        (self.bin / mode).chmod(0o755)
        identity = self.root / 'identity'
        identity.write_text('test identity')
        self.configure(BACKUP_ENCRYPT=mode, BACKUP_RECIPIENT='test-recipient',
                       BACKUP_PLAINTEXT_ACK='0', BACKUP_IDENTITY=str(identity))

    def test_unconfigured_backup_refuses_before_stopping(self):
        self.configure(BACKUP_PLAINTEXT_ACK='0')
        result = self.run_script('backup.sh')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('BACKUP_RECIPIENT', result.stderr)
        self.assertNotIn('"stop"', (self.fake / 'calls.jsonl').read_text())
        self.assertFalse((self.root / 'backups').exists())
        self.assert_no_plaintext_scratch()

    def test_explicit_plaintext_ack_warns(self):
        result = self.run_script('backup.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('!!! WARNING: 明文备份含签名私钥', result.stderr)
        self.assertEqual(len(list((self.root / 'backups').glob('*.tar'))), 1)
        self.assert_no_plaintext_scratch()

    def test_age_and_gpg_roundtrip_leaves_only_encrypted_archives(self):
        self.db.close()
        for mode in ('age', 'gpg'):
            with self.subTest(mode=mode):
                self.crypto(mode)
                result = self.run_script('backup.sh')
                self.assertEqual(result.returncode, 0, result.stderr)
                archive = sorted((self.root / 'backups').glob('*.' + mode))[-1]
                self.assertEqual(archive.stat().st_mode & 0o777, 0o600)
                self.assert_no_plaintext_scratch()
                with closing(sqlite3.connect(self.fake / 'data/collab.db')) as db, db:
                    db.execute("UPDATE accounts SET name='changed'")
                result = self.run_script('restore.sh', archive)
                self.assertEqual(result.returncode, 0, result.stderr)
                with closing(sqlite3.connect(self.fake / 'data/collab.db')) as db:
                    self.assertEqual(db.execute('SELECT name FROM accounts').fetchone()[0], 'before')
                self.assert_no_plaintext_scratch()
                self.assertTrue(json.loads(self.state.read_text())['running'])
                self.assertFalse(list((self.root / 'backups').glob('*.tar')))
        calls = [json.loads(line) for line in (self.fake / 'crypto.jsonl').read_text().splitlines()]
        for mode in ('age', 'gpg'):
            self.assertTrue(any(c[0] == mode and '--recipient' in c and 'test-recipient' in c for c in calls))
            self.assertTrue(any(c[0] == mode and '--decrypt' in c for c in calls))
        self.assertTrue(any(c[0] == 'age' and '--identity' in c for c in calls))

    def test_encryption_failure_cleans_partial_and_restarts(self):
        self.crypto('age')
        # Even an explicit plaintext ACK cannot downgrade a failed encryption command.
        self.configure(BACKUP_PLAINTEXT_ACK='1')
        self.env['FAKE_CRYPTO_FAIL'] = '1'
        result = self.run_script('backup.sh')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(json.loads(self.state.read_text())['running'])
        self.assertEqual(list((self.root / 'backups').iterdir()), [])
        self.assert_no_plaintext_scratch()

    def test_decryption_failure_cleans_plaintext_before_docker(self):
        self.crypto('age')
        result = self.run_script('backup.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = next((self.root / 'backups').glob('*.age'))
        before = (self.fake / 'calls.jsonl').read_text()
        self.env['FAKE_CRYPTO_FAIL'] = '1'
        result = self.run_script('restore.sh', archive)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.fake / 'calls.jsonl').read_text(), before)
        self.assert_no_plaintext_scratch()

    def assert_crypto_timeout(self, result, mode, operation):
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(f'BackupCryptoTimeout: {mode} {operation}超时', result.stderr)
        self.assertIn('BACKUP_CRYPTO_TIMEOUT_SECS=1', result.stderr)
        pid = int((self.fake / 'crypto.pid').read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)
        self.assert_no_plaintext_scratch()

    def test_encryption_timeout_cleans_partial_and_restarts(self):
        for mode in ('age', 'gpg'):
            with self.subTest(mode=mode):
                self.crypto(mode)
                self.configure(BACKUP_CRYPTO_TIMEOUT_SECS='1', BACKUP_PLAINTEXT_ACK='1')
                self.env['FAKE_CRYPTO_HANG'] = '1'
                result = self.run_script('backup.sh')
                self.assert_crypto_timeout(result, mode, '加密')
                self.assertTrue(json.loads(self.state.read_text())['running'])
                self.assertEqual(list((self.root / 'backups').iterdir()), [])

    def test_decryption_timeout_cleans_plaintext_before_docker(self):
        for mode in ('age', 'gpg'):
            with self.subTest(mode=mode):
                self.crypto(mode)
                self.configure(BACKUP_CRYPTO_TIMEOUT_SECS='1')
                self.env.pop('FAKE_CRYPTO_HANG', None)
                result = self.run_script('backup.sh')
                self.assertEqual(result.returncode, 0, result.stderr)
                archive = next((self.root / 'backups').glob('*.' + mode))
                original = archive.read_bytes()
                before = (self.fake / 'calls.jsonl').read_text()
                self.env['FAKE_CRYPTO_HANG'] = '1'
                result = self.run_script('restore.sh', archive)
                self.assert_crypto_timeout(result, mode, '解密')
                self.assertEqual((self.fake / 'calls.jsonl').read_text(), before)
                self.assertEqual(archive.read_bytes(), original)
                self.assertTrue(json.loads(self.state.read_text())['running'])

    def test_restore_pubkey_request_timeout_is_capped(self):
        archive = self.backup()
        for budget, request_limit in ((1, 1), (5, 5), (10, 10), (30, 10), (300, 10)):
            with self.subTest(budget=budget):
                self.configure(RESTORE_VERIFY_TIMEOUT_SECS=str(budget))
                result = self.run_script('restore.sh', archive)
                self.assertEqual(result.returncode, 0, result.stderr)
                calls = [json.loads(line) for line in
                         (self.fake / 'calls.jsonl').read_text().splitlines()]
                request = [call for call in calls if 'curl' in call][-1]
                self.assertEqual(request[request.index('--max-time') + 1], str(request_limit))
                self.assertEqual(request[request.index('--retry-max-time') + 1], str(budget))
                self.assert_no_plaintext_scratch()

    def test_restore_missing_pin_refuses_before_docker(self):
        archive = self.backup()
        before = (self.fake / 'calls.jsonl').read_text()
        self.configure(EXPECTED_FINGERPRINT='')
        result = self.run_script('restore.sh', archive)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('BLOCKED', result.stderr)
        self.assertIn('EXPECTED_FINGERPRINT', result.stderr)
        self.assertEqual((self.fake / 'calls.jsonl').read_text(), before)

    def test_restore_mismatch_stops_authority_even_with_ack(self):
        archive = self.backup()
        self.configure(EXPECTED_FINGERPRINT='a' * 64, RESTORE_UNPINNED_ACK='1')
        result = self.run_script('restore.sh', archive)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(json.loads(self.state.read_text())['running'])
        self.assertIn('权威完整指纹与预期不一致', result.stderr)
        self.assertIn('停止 authority', result.stderr)
        self.assert_no_plaintext_scratch()

    def test_restore_failed_pubkey_fetch_stops_authority(self):
        archive = self.backup()
        result = self.run_script('restore.sh', archive, fail='exec')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(json.loads(self.state.read_text())['running'])
        self.assert_no_plaintext_scratch()

    def test_restore_ack_warns_and_prints_actual_fingerprint(self):
        archive = self.backup()
        self.configure(EXPECTED_FINGERPRINT='', RESTORE_UNPINNED_ACK='1')
        result = self.run_script('restore.sh', archive)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('!!! WARNING: RESTORE_UNPINNED_ACK=1', result.stderr)
        self.assertIn(self.pubkey['fingerprint'], result.stdout)
        self.assertIn('身份尚未核实', result.stdout)

    def test_restore_checks_previously_stopped_authority(self):
        archive = self.backup()
        self.state.write_text(json.dumps({'running': False}))
        result = self.run_script('restore.sh', archive)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('EXPECTED_FINGERPRINT 一致', result.stdout)
        self.assertFalse(json.loads(self.state.read_text())['running'])


if __name__ == '__main__':
    unittest.main()
