#!/usr/bin/env python3
"""Deployment inputs: strict dotenv subset, no shell execution; stdlib only."""
import json
import math
import os
import re
import sys
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]


def read_env(path):
    result = {}
    for number, raw in enumerate(Path(path).read_text().splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith('#'):
            continue
        key, sep, value = line.partition('=')
        if not sep or not re.fullmatch(r'[A-Z][A-Z0-9_]*', key):
            raise ValueError(f'{path}:{number}: 需要 KEY=value')
        if key in result:
            raise ValueError(f'{path}:{number}: 重复字段 {key}')
        if value.startswith("'") and value.endswith("'"):
            value = value[1:-1]
        if any(c in value for c in '\n\r\x00'):
            raise ValueError(f'{key}: 不允许控制字符')
        result[key] = value
    return result


def load(path):
    defaults = read_env(ROOT / '.env.example')
    supplied = read_env(path)
    unknown = supplied.keys() - defaults.keys()
    if unknown:
        raise ValueError(f'未知配置字段: {sorted(unknown)}')
    return defaults | supplied


def integer(env, key, maximum=2**32 - 1):
    value = env[key]
    if not re.fullmatch(r'[1-9][0-9]*', value) or int(value) > maximum:
        raise ValueError(f'{key}: 需要 1..{maximum} 的整数')
    return int(value)


def validate(env):
    for key in ('AUTHORITY_DOMAIN', 'RELAY_DOMAIN', 'SMTP_HOST'):
        value = env[key]
        if key == 'SMTP_HOST' and not value:
            continue
        labels = value.split('.')
        if len(value) > 253 or len(labels) < 2 or not all(
            re.fullmatch(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?', label)
            for label in labels
        ):
            raise ValueError(f'{key}: 需要小写 DNS 主机名')
    if env['AUTHORITY_DOMAIN'] == env['RELAY_DOMAIN']:
        raise ValueError('权威与 relay 域名必须不同')
    if not re.fullmatch(r'[A-Za-z0-9_.+%-]+@[A-Za-z0-9.-]+', env['ACME_EMAIL']):
        raise ValueError('ACME_EMAIL 格式无效')
    for key in ('AUTHORITY_IMAGE', 'RELAY_IMAGE'):
        if not re.fullmatch(r'[a-zA-Z0-9._/:-]+:[a-zA-Z0-9._-]+@sha256:[0-9a-f]{64}', env[key]):
            raise ValueError(f'BLOCKED: {key} 需要版本标签与 sha256 digest')
    for key in ('AUTHORITY_IMAGE', 'RELAY_IMAGE', 'CADDY_IMAGE', 'GATEWAY_IMAGE'):
        value = env[key]
        if not re.fullmatch(r'[a-zA-Z0-9._/:-]+:[a-zA-Z0-9._-]+(?:@sha256:[0-9a-f]{64})?', value):
            raise ValueError(f'{key}: 镜像必须固定版本')
        if value.split('@')[0].rsplit(':', 1)[1] in ('latest', 'main', 'stable', 'alpine'):
            raise ValueError(f'{key}: 不允许浮动版本')
    for key in ('ACCOUNT_OPEN_SIGNUP', 'SMOKE_SIGNUP'):
        if env[key] not in ('0', '1'):
            raise ValueError(f'{key}: 仅允许 0/1')
    if env['RELAY_FALLBACK'] not in ('true', 'false'):
        raise ValueError('RELAY_FALLBACK: 仅允许 true/false')
    limits = {'SMTP_PORT': 65535, 'REVOCATION_POLL_SECS': 3600,
              'REVOCATION_MAX_AGE_SECS': 86400, 'NEW_DEVICE_WINDOW_HOURS': 720,
              'APPROVAL_TIMEOUT_SECS': 3600, 'SMOKE_TIMEOUT_SECS': 120,
              'LOG_MAX_FILES': 100}
    for key in ('AUTH_REQUESTS_PER_10S', 'RELAY_ACCEPT_PER_SEC', 'RELAY_ACCEPT_BURST',
                'RELAY_RX_BYTES_PER_SEC', 'RELAY_RX_BURST_BYTES'):
        limits[key] = 2**32 - 1
    for key, limit in limits.items():
        integer(env, key, limit)
    for service in ('AUTHORITY', 'RELAY', 'CADDY', 'GATEWAY'):
        if not re.fullmatch(r'[1-9][0-9]*[mg]', env[service + '_MEMORY']):
            raise ValueError(f'{service}_MEMORY: 需要正整数 m/g')
        cpu = float(env[service + '_CPUS'])
        if not math.isfinite(cpu) or not 0 < cpu <= 64:
            raise ValueError(f'{service}_CPUS: 需要 (0,64] 数字')
    if not re.fullmatch(r'[1-9][0-9]*m', env['LOG_MAX_SIZE']):
        raise ValueError('LOG_MAX_SIZE: 需要正整数 m')
    if not env['BACKUP_DIR'].strip():
        raise ValueError('BACKUP_DIR 不能为空')
    smtp = [env[k] for k in ('SMTP_HOST', 'SMTP_USER', 'SMTP_PASS', 'SMTP_FROM')]
    if any(smtp) and not all(smtp):
        raise ValueError('SMTP_HOST/USER/PASS/FROM 必须全部填写或全部留空')
    if bool(env['SMOKE_USERNAME']) != bool(env['SMOKE_PASSWORD']):
        raise ValueError('自检账号与密码必须同时提供')
    if env['SMOKE_SIGNUP'] == '1' and not env['SMOKE_USERNAME']:
        raise ValueError('SMOKE_SIGNUP=1 需要测试账号')
    fp = env['EXPECTED_FINGERPRINT']
    if fp and not re.fullmatch(r'[0-9a-f]{64}', fp):
        raise ValueError('EXPECTED_FINGERPRINT 必须为完整小写 SHA-256，不能为 kid')
    for url in relay_urls(env):
        parsed = urlparse(url)
        if (parsed.scheme != 'https' or not parsed.hostname or parsed.username or
                parsed.password or parsed.query or parsed.fragment or parsed.path not in ('', '/')):
            raise ValueError('RELAY_URLS 需要逗号分隔的 HTTPS 源站 URL')
        _ = parsed.port


def relay_urls(env):
    return [v.strip() for v in env['RELAY_URLS'].split(',') if v.strip()] or [
        'https://' + env['RELAY_DOMAIN']]


def interconnect(env, client=False):
    return {
        'authority': {'url': 'https://' + env['AUTHORITY_DOMAIN'] if client else None,
                      'publicKeyFingerprints': [env['EXPECTED_FINGERPRINT']] if client else []},
        'relay': {'urls': relay_urls(env), 'fallback': env['RELAY_FALLBACK'] == 'true'},
        'security': {'revocationPollSecs': int(env['REVOCATION_POLL_SECS']),
                     'revocationMaxAgeSecs': int(env['REVOCATION_MAX_AGE_SECS']),
                     'newDeviceWindowHours': int(env['NEW_DEVICE_WINDOW_HOURS']),
                     'approvalTimeoutSecs': int(env['APPROVAL_TIMEOUT_SECS'])},
    }


def prepare(env):
    validate(env)
    out = ROOT / '.generated'
    out.mkdir(mode=0o700, exist_ok=True)
    for name in ('Caddyfile', 'haproxy.cfg', 'relay.toml'):
        content = (ROOT / 'templates' / name).read_text()
        for key in re.findall(r'@([A-Z_0-9]+)@', content):
            content = content.replace('@' + key + '@', env[key])
        (out / name).write_text(content)
        (out / name).chmod(0o644)
    (out / 'authority.json').write_text(json.dumps(interconnect(env), indent=2) + '\n')
    (out / 'authority.json').chmod(0o644)
    # Compose 单引号值保持 $ 为字面量，不让 SMTP 密码进入变量插值。
    content = ''.join(k + "='" + v.replace("'", "\\'") + "'\n" for k, v in env.items())
    target = out / 'compose.env'
    target.touch(mode=0o600, exist_ok=True)
    target.chmod(0o600)
    target.write_text(content)
    client = out / 'client-interconnect.json'
    if env['EXPECTED_FINGERPRINT']:
        client.write_text(json.dumps(interconnect(env, True), indent=2) + '\n')
    else:
        client.unlink(missing_ok=True)


def main():
    action, path, *args = sys.argv[1:]
    env = load(path)
    if action == 'prepare':
        prepare(env)
    elif action == 'compose':
        validate(env)
        command = ['docker', 'compose', '--project-directory', str(ROOT),
                   '--env-file', str(ROOT / '.generated/compose.env'),
                   '-f', str(ROOT / 'docker-compose.yml'), *args]
        # Explicit task inputs override ambient shell variables, including project identity.
        os.execvpe('docker', command, os.environ | env | {'COMPOSE_PROJECT_NAME': 'polaris-singapore'})
    elif action == 'value':
        print(env[args[0]])
    else:
        raise ValueError('未知动作')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        sys.exit(str(error))
