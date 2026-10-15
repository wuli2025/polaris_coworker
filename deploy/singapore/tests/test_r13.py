"""Proxy trust and bounded delivery are generated from the same validated deployment inputs."""
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yaml

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import config


class HardeningTests(unittest.TestCase):
    def inputs(self):
        env = config.read_env(ROOT / '.env.example')
        for key in ('AUTHORITY_IMAGE', 'RELAY_IMAGE'):
            env[key] = 'example/service:1.0@sha256:' + 'a' * 64
        return env

    def test_generated_authority_trusts_both_internal_proxy_hops(self):
        env = self.inputs()
        env.update(EDGE_SUBNET='172.29.10.0/24', ACCOUNT_SUBNET='172.29.11.0/24',
                   MAIL_DELIVERY_CAPACITY='8', MAIL_DELIVERY_CONCURRENCY='3',
                   MAIL_DELIVERY_TIMEOUT_SECS='12')
        config.validate(env)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'templates').symlink_to(ROOT / 'templates', target_is_directory=True)
            with patch.object(config, 'ROOT', root):
                config.prepare(env)
            generated = json.loads((root / '.generated/authority.json').read_text())
            self.assertEqual(generated['ingress']['trustedProxyCidrs'],
                             ['172.29.10.0/24', '172.29.11.0/24'])
            self.assertEqual(generated['mailDelivery'],
                             {'capacity': 8, 'concurrency': 3, 'timeoutSecs': 12})
            self.assertIn('header_up X-Forwarded-For {remote_host}',
                          (root / '.generated/Caddyfile').read_text())
        self.assertNotIn('ingress', config.interconnect(env, client=True))
        raw = (ROOT / 'docker-compose.yml').read_text()
        for key in ('EDGE_SUBNET', 'ACCOUNT_SUBNET'):
            default = config.read_env(ROOT / '.env.example')[key]
            raw = raw.replace('${' + key + ':-' + default + '}', env[key])
        compose = yaml.safe_load(raw)
        self.assertEqual(compose['networks']['edge']['ipam']['config'][0]['subnet'], env['EDGE_SUBNET'])
        self.assertEqual(compose['networks']['account']['ipam']['config'][0]['subnet'], env['ACCOUNT_SUBNET'])

    def test_invalid_network_and_delivery_inputs_are_rejected(self):
        for changes in [
            {'EDGE_SUBNET': 'invalid'}, {'EDGE_SUBNET': '172.30.80.1/24'},
            {'EDGE_SUBNET': '0.0.0.0/0'}, {'EDGE_SUBNET': '172.30.81.0/24'},
            {'MAIL_DELIVERY_CAPACITY': '0'}, {'MAIL_DELIVERY_CONCURRENCY': '65'},
            {'MAIL_DELIVERY_TIMEOUT_SECS': '301'}, {'MAIL_DELIVERY_TIMEOUT_SECS': '1.5'},
            {'MAIL_DELIVERY_CAPACITY': '1', 'MAIL_DELIVERY_CONCURRENCY': '2'},
        ]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                config.validate(self.inputs() | changes)


if __name__ == '__main__':
    unittest.main()
