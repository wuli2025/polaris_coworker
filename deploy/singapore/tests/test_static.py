#!/usr/bin/env python3
"""Fallback checks; needs preinstalled PyYAML, never installs a dependency."""
import re
import sys
import tomllib
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import config


class StaticTests(unittest.TestCase):
    def test_compose_exposure_and_lifecycle(self):
        compose = yaml.safe_load((ROOT / 'docker-compose.yml').read_text())
        services = compose['services']
        self.assertEqual(set(services), {'authority', 'iroh-relay', 'caddy', 'gateway'})
        self.assertNotIn('ports', services['authority'])
        self.assertNotIn('ports', services['caddy'])
        self.assertEqual(services['iroh-relay']['ports'], ['7824:7824/udp'])
        self.assertEqual(services['gateway']['ports'], ['80:8080/tcp', '443:8443/tcp'])
        self.assertEqual(services['authority']['environment']['POLARIS_AUTHORITY_ONLY'], '1')
        for service in services.values():
            self.assertEqual(service['restart'], 'unless-stopped')
            for field in ('healthcheck', 'logging', 'cpus', 'mem_limit', 'pids_limit'):
                self.assertIn(field, service)
        known = config.read_env(ROOT / '.env.example')
        variables = re.findall(r'\$\{([A-Z0-9_]+)', (ROOT / 'docker-compose.yml').read_text())
        self.assertFalse(set(variables) - known.keys())

    def test_relay_config_matches_pinned_api(self):
        env = config.read_env(ROOT / '.env.example')
        raw = (ROOT / 'templates/relay.toml').read_text()
        for key in re.findall(r'@([A-Z_0-9]+)@', raw):
            raw = raw.replace('@' + key + '@', env[key])
        relay = tomllib.loads(raw)
        self.assertTrue(relay['enable_quic_addr_discovery'])
        self.assertEqual(relay['tls']['cert_mode'], 'LetsEncrypt')
        self.assertEqual(relay['tls']['quic_bind_addr'], '0.0.0.0:7824')
        self.assertNotIn('https_bind_addr', relay)  # 1.0.1 requires it INSIDE tls.
        lock = tomllib.loads((ROOT.parents[1] / 'src-tauri/Cargo.lock').read_text())
        packages = {item['name']: item for item in lock['package'] if item['name'] in ('iroh', 'iroh-relay')}
        self.assertEqual(packages['iroh']['version'], '1.0.1')
        relay_package = packages['iroh-relay']
        self.assertEqual(relay_package['version'], '1.0.1')
        self.assertIn(relay_package['checksum'], (ROOT / 'Dockerfile.relay').read_text())

    def test_file_limits(self):
        for path in [*ROOT.rglob('*'), ROOT.parents[1] / 'docs/feature-tree.md']:
            if not path.is_file() or any(p in path.parts for p in ('.generated', '__pycache__')):
                continue
            lines = path.read_text().splitlines()
            self.assertLessEqual(len(lines), 500, str(path))
            self.assertTrue(all(len(line) <= 500 for line in lines), str(path))


if __name__ == '__main__':
    unittest.main()
