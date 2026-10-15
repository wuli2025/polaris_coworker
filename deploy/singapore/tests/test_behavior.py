#!/usr/bin/env python3
import base64
import hashlib
import io
import json
import os
import shutil
import sqlite3
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import config
import probe
import snapshot


def inputs():
    env = config.read_env(ROOT / '.env.example')
    for key in ('AUTHORITY_IMAGE', 'RELAY_IMAGE'):
        env[key] = 'example.invalid/test:1.0.1@sha256:' + '1' * 64
    return env


class ConfigurationTests(unittest.TestCase):
    def test_defaults_and_custom_relays(self):
        env = inputs()
        config.validate(env)
        self.assertEqual(config.relay_urls(env), ['https://relay.example.invalid'])
        env['RELAY_URLS'] = 'https://one.example.invalid,https://two.example.invalid'
        self.assertEqual(len(config.interconnect(env)['relay']['urls']), 2)
        self.assertFalse(config.interconnect(env)['relay']['fallback'])

    def test_bad_inputs_fail_closed(self):
        for key, value in [('AUTHORITY_IMAGE', 'repo:latest'), ('RELAY_IMAGE', ''),
                           ('AUTHORITY_DOMAIN', 'bad\nrespond 200'),
                           ('RELAY_FALLBACK', '1'), ('SMTP_PORT', '65536'),
                           ('SMTP_USER', 'partial'), ('AUTH_REQUESTS_PER_10S', '0'),
                           ('REVOCATION_POLL_SECS', '1.5'), ('EXPECTED_FINGERPRINT', 'abcd'),
                           ('BACKUP_ENCRYPT', 'openssl'), ('BACKUP_RECIPIENT', '--output=x'),
                           ('BACKUP_PLAINTEXT_ACK', 'true'), ('BACKUP_IDENTITY', 'relative'),
                           ('RESTORE_UNPINNED_ACK', 'yes'), ('RESTORE_VERIFY_TIMEOUT_SECS', '301'),
                           ('RELAY_URLS', 'http://relay.example.invalid'),
                           ('AUTHORITY_CPUS', 'nan'), ('SMOKE_SIGNUP', '1')]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                config.validate(inputs() | {key: value})

    def test_crypto_timeout_defaults_and_validation(self):
        key = 'BACKUP_CRYPTO_TIMEOUT_SECS'
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / '.env'
            path.write_text('')
            self.assertEqual(config.load(path)[key], '300')
        for value in ('1', '300', '600', '86400'):
            with self.subTest(value=value):
                config.validate(inputs() | {key: value})
        for value in ('', '0', '-1', '1.5', 'true', 'nan', 'inf', '86401'):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, key):
                config.validate(inputs() | {key: value})

    def test_generation_no_secrets_in_proxy_configs_and_no_stale_pin(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copytree(ROOT / 'templates', root / 'templates')
            env = inputs() | {'EXPECTED_FINGERPRINT': 'a' * 64}
            with patch.object(config, 'ROOT', root):
                config.prepare(env)
                result = json.loads((root / '.generated/client-interconnect.json').read_text())
                self.assertEqual(result['authority']['publicKeyFingerprints'], ['a' * 64])
                config.prepare(env | {'EXPECTED_FINGERPRINT': ''})
                self.assertFalse((root / '.generated/client-interconnect.json').exists())
                self.assertEqual((root / '.generated/compose.env').stat().st_mode & 0o777, 0o600)
                self.assertEqual((root / '.generated/relay.toml').stat().st_mode & 0o777, 0o644)

    def test_compose_ignores_ambient_image_and_project_override(self):
        env = inputs()
        argv = ['config.py', 'compose', 'unused', 'config', '--quiet']
        with patch.object(config, 'load', return_value=env), patch.object(sys, 'argv', argv), \
                patch.dict(os.environ, {'AUTHORITY_IMAGE': 'bad:latest', 'COMPOSE_PROJECT_NAME': 'wrong'}), \
                patch.object(os, 'execvpe') as execute:
            config.main()
            passed = execute.call_args.args[2]
            self.assertEqual(passed['AUTHORITY_IMAGE'], env['AUTHORITY_IMAGE'])
            self.assertEqual(passed['COMPOSE_PROJECT_NAME'], 'polaris-singapore')

    def test_dotenv_never_executes_shell_and_rejects_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / '.env'
            path.write_text("SMTP_PASS='$(touch /never-created)$secret'\n")
            self.assertEqual(config.read_env(path)['SMTP_PASS'], '$(touch /never-created)$secret')
            path.write_text('UNKNOWN=1\n')
            with self.assertRaises(ValueError):
                config.load(path)


class ProbeTests(unittest.TestCase):
    def document(self):
        # RFC 8032 public test vector (public key only, not a deployment secret).
        key = bytes.fromhex('d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a')
        return {'alg': 'ed25519', 'publicKey': base64.urlsafe_b64encode(key).decode().rstrip('='),
                'fingerprint': hashlib.sha256(key).hexdigest()}

    def test_full_fingerprint_and_mismatch(self):
        doc = self.document()
        self.assertEqual(probe.fingerprint(doc, doc['fingerprint']), doc['fingerprint'])
        for expected in ('a' * 64, doc['fingerprint'][:16]):
            with self.assertRaises(ValueError):
                probe.fingerprint(doc, expected)
        with self.assertRaises(ValueError):
            probe.fingerprint(doc | {'fingerprint': 'b' * 64})

    def test_udp_requires_matching_quic_reply(self):
        dcid, scid = b'd' * 8, b's' * 8
        reply = b'\x80' + b'\0' * 4 + b'\x08' + scid + b'\x08' + dcid + struct.pack('!I', 1)
        self.assertEqual(len(probe.vn_request(dcid, scid)), 1200)
        self.assertTrue(probe.verify_vn(reply, dcid, scid))
        for bad in (b'', b'hello', reply[:-1], reply.replace(scid, b'x' * 8),
                    reply[:-4] + struct.pack('!I', 0xFAFAFAFA)):
            self.assertFalse(probe.verify_vn(bad, dcid, scid))

    def test_smoke_pin_failure_precedes_credentials(self):
        doc = self.document()
        with patch.object(probe, 'load', return_value=inputs() | {
            'EXPECTED_FINGERPRINT': 'a' * 64, 'SMOKE_USERNAME': 'test', 'SMOKE_PASSWORD': 'test'
        }), patch.object(probe, 'json_request', side_effect=[
            (200, {'mode': 'authority'}), (200, doc)
        ]) as request:
            with self.assertRaises(ValueError):
                probe.smoke('unused')
            self.assertEqual(request.call_count, 2)

    def test_optional_signup_is_repeatable(self):
        doc = self.document()
        env = inputs() | {'EXPECTED_FINGERPRINT': doc['fingerprint'],
                          'SMOKE_USERNAME': 'test', 'SMOKE_PASSWORD': 'test', 'SMOKE_SIGNUP': '1'}
        for responses, count in [([(200, {'assertion': 'PA1.test'})], 3),
                                  ([(400, {}), (200, {'assertion': 'PA1.test'}),
                                    (200, {'assertion': 'PA1.test'})], 5)]:
            with patch.object(probe, 'load', return_value=env), patch.object(
                probe, 'json_request', side_effect=[(200, {'mode': 'authority'}), (200, doc)] + responses
            ) as request:
                probe.smoke('unused', True)
                self.assertEqual(request.call_count, count)


class MaintenanceFixture:
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.package = self.root / 'package'
        shutil.copytree(ROOT, self.package, ignore=shutil.ignore_patterns('.generated', '__pycache__'))
        self.fake = self.root / 'fake'
        self.fake.mkdir()
        (self.fake / 'data').mkdir()
        self.state = self.fake / 'state.json'
        self.state.write_text(json.dumps({'running': True}))
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        shutil.copy(ROOT / 'tests/fake_docker.py', self.bin / 'docker')
        (self.bin / 'docker').chmod(0o755)
        self.env = os.environ | {'PATH': str(self.bin) + ':' + os.environ['PATH'],
                                'FAKE_DOCKER_ROOT': str(self.fake),
                                'SG_ENV_FILE': str(self.package / '.env'),
                                'TMPDIR': str(self.root / 'scratch')}
        (self.root / 'scratch').mkdir()
        self.pubkey = ProbeTests().document()
        (self.fake / 'pubkey.json').write_text(json.dumps(self.pubkey))
        values = inputs() | {'BACKUP_DIR': str(self.root / 'backups'),
                             'EXPECTED_FINGERPRINT': self.pubkey['fingerprint'],
                             'BACKUP_PLAINTEXT_ACK': '1'}
        (self.package / '.env').write_text(''.join(f'{k}={v}\n' for k, v in values.items()))
        # Deliberately synthetic invalid-for-production seed; temporary directory only.
        (self.fake / 'data/account_authority.key').write_bytes(base64.urlsafe_b64encode(b'\x00' * 32).rstrip(b'='))
        self.db = sqlite3.connect(self.fake / 'data/collab.db')
        self.db.execute('PRAGMA journal_mode=WAL')
        self.db.execute('CREATE TABLE accounts (name TEXT)')
        self.db.execute("INSERT INTO accounts VALUES ('before')")
        self.db.commit()

    def tearDown(self):
        self.db.close()
        self.temp.cleanup()

    def run_script(self, name, *args, fail=''):
        return subprocess.run(['bash', str(self.package / 'scripts' / name), *map(str, args)],
                              env=self.env | {'FAKE_FAIL': fail}, capture_output=True, text=True)

    def backup(self):
        result = self.run_script('backup.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        return sorted((self.root / 'backups').glob('*.tar'))[-1]

    def configure(self, **updates):
        path = self.package / '.env'
        values = config.read_env(path) | updates
        path.write_text(''.join(f'{k}={v}\n' for k, v in values.items()))

    def assert_no_plaintext_scratch(self):
        self.assertEqual(list((self.root / 'scratch').iterdir()), [])
        backups = self.root / 'backups'
        if backups.exists():
            self.assertFalse(any(p.is_dir() or p.name.endswith('.partial') for p in backups.iterdir()))


class MaintenanceTests(MaintenanceFixture, unittest.TestCase):
    def test_backup_includes_committed_wal_and_preserves_running_state(self):
        archive = self.backup()
        self.assertTrue(json.loads(self.state.read_text())['running'])
        with tarfile.open(archive) as tar:
            restored = self.root / 'copy.db'
            restored.write_bytes(tar.extractfile('collab.db').read())
        with sqlite3.connect(restored) as db:
            self.assertEqual(db.execute('SELECT name FROM accounts').fetchone()[0], 'before')
        self.assertEqual(snapshot.verify(archive)['format'], 1)
        self.state.write_text(json.dumps({'running': False}))
        self.backup()
        self.assertFalse(json.loads(self.state.read_text())['running'])

    def test_backup_copy_failure_restarts_original_service(self):
        result = self.run_script('backup.sh', fail='cp')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(json.loads(self.state.read_text())['running'])
        self.assertEqual(list((self.root / 'backups').glob('*.tar')), [])
        self.assert_no_plaintext_scratch()

    def test_restore_repeated_and_failed_extraction_stays_stopped(self):
        archive = self.backup()
        self.db.close()
        for _ in range(2):
            result = self.run_script('restore.sh', archive)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(json.loads(self.state.read_text())['running'])
            self.assertIn('EXPECTED_FINGERPRINT 一致', result.stdout)
            self.assert_no_plaintext_scratch()
        result = self.run_script('restore.sh', archive, fail='extract')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(json.loads(self.state.read_text())['running'])
        self.assert_no_plaintext_scratch()

    def test_tampered_or_traversal_archive_rejected_before_docker(self):
        archive = self.backup()
        for member in ('../collab.db', 'collab.db'):
            bad = self.root / 'bad.tar'
            with tarfile.open(archive) as source, tarfile.open(bad, 'w') as dest:
                for info in source.getmembers():
                    data = source.extractfile(info).read()
                    if info.name == 'collab.db':
                        info.name = member
                        data = b'corrupt'
                        info.size = len(data)
                    dest.addfile(info, io.BytesIO(data))
            before = (self.fake / 'calls.jsonl').read_text()
            result = self.run_script('restore.sh', bad)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual((self.fake / 'calls.jsonl').read_text(), before)

    def test_init_missing_key_never_regenerates_existing_identity(self):
        (self.fake / 'data/account_authority.key').unlink()
        result = self.run_script('init-keys.sh')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('"up"', (self.fake / 'calls.jsonl').read_text())

    def test_upgrade_success_order_and_scope(self):
        # Replace only the HTTPS probe in this disposable package; no public network.
        (self.package / 'scripts/probe.py').write_text(
            "import os, pathlib\n"
            "p = pathlib.Path(os.environ['FAKE_DOCKER_ROOT']) / 'calls.jsonl'\n"
            "with p.open('a') as f: f.write('[\"probe\"]\\n')\n"
        )
        result = self.run_script('upgrade.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in (self.fake / 'calls.jsonl').read_text().splitlines()]
        actions = []
        for call in calls:
            if call[0] == 'compose':
                call = call[1:]
                while call and call[0] in ('--project-directory', '--env-file', '-f'):
                    call = call[2:]
            if call[0] in ('pull', 'stop', 'cp', 'start', 'up', 'probe'):
                actions.append(call[0])
        self.assertEqual(actions, ['pull', 'stop', 'cp', 'start', 'up', 'probe'])
        self.assertIn('previous', (self.package / '.generated/previous-authority-image.txt').read_text())

    def test_upgrade_pull_failure_does_not_stop_authority(self):
        result = self.run_script('upgrade.sh', fail='pull')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(json.loads(self.state.read_text())['running'])
        self.assertNotIn('"stop"', (self.fake / 'calls.jsonl').read_text())


if __name__ == '__main__':
    unittest.main()
