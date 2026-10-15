#!/usr/bin/env python3
"""HTTPS account API and UDP QUIC version negotiation probes; no third-party deps."""
import base64
import hashlib
import json
import secrets
import socket
import struct
import sys
import urllib.error
import urllib.request

from config import load, validate


def fingerprint(document, expected=''):
    if document.get('alg') != 'ed25519':
        raise ValueError('权威签名算法不是 ed25519')
    raw = document['publicKey']
    key = base64.b64decode(raw + '=' * (-len(raw) % 4), altchars=b'-_', validate=True)
    if len(key) != 32:
        raise ValueError('公钥长度错误')
    actual = hashlib.sha256(key).hexdigest()
    if document.get('fingerprint') != actual:
        raise ValueError('返回的 fingerprint 与公钥不一致')
    if expected and actual != expected:
        raise ValueError('权威完整指纹与预期不一致')
    return actual


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError('自检拒绝重定向，防止测试凭据转发到其他域名')


def request(url, timeout, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, headers={'Content-Type': 'application/json'})
    opener = urllib.request.build_opener(NoRedirect())
    try:
        with opener.open(req, timeout=timeout) as response:
            payload = response.read(1024 * 1024)
            return response.status, payload
    except urllib.error.HTTPError as error:
        # 不输出可能包含凭据/断言的响应。
        return error.code, error.read(1024 * 1024)


def json_request(url, timeout, body=None):
    status, payload = request(url, timeout, body)
    try:
        result = json.loads(payload)
    except (ValueError, UnicodeError) as error:
        raise ValueError(f'API 未返回 JSON (HTTP {status})') from error
    if not isinstance(result, dict):
        raise ValueError('API 返回类型不是 object')
    return status, result


def vn_request(dcid, scid):
    # RFC 9000 保留版本 0x?a?a?a?a，触发版本协商；Initial 至少 1200 字节。
    header = b'\xc0' + struct.pack('!I', 0xFAFAFAFA)
    header += bytes([len(dcid)]) + dcid + bytes([len(scid)]) + scid
    return header + b'\x00' * (1200 - len(header))


def verify_vn(data, dcid, scid):
    if len(data) < 7 or not data[0] & 0x80 or data[1:5] != b'\0' * 4:
        return False
    size = data[5]
    pos = 6 + size
    if pos >= len(data) or data[6:pos] != scid:
        return False
    size2 = data[pos]
    end = pos + 1 + size2
    if data[pos + 1:end] != dcid or end >= len(data) or (len(data) - end) % 4:
        return False
    versions = struct.unpack('!' + 'I' * ((len(data) - end) // 4), data[end:])
    return 1 in versions and 0xFAFAFAFA not in versions


def quic_probe(host, timeout):
    errors = []
    for family, socktype, proto, _, address in socket.getaddrinfo(
        host, 7824, type=socket.SOCK_DGRAM
    ):
        try:
            with socket.socket(family, socktype, proto) as sock:
                sock.settimeout(timeout)
                sock.connect(address)
                dcid, scid = secrets.token_bytes(8), secrets.token_bytes(8)
                sock.send(vn_request(dcid, scid))
                data = sock.recv(65535)
                if not verify_vn(data, dcid, scid):
                    raise ValueError('没有匹配本次连接 ID 的 QUIC 版本协商响应')
            return
        except (OSError, ValueError) as error:
            errors.append(type(error).__name__)
    raise ValueError('UDP 7824 QUIC 探测失败: ' + ', '.join(errors))


def smoke(path, authority_only=False):
    env = load(path)
    validate(env)
    expected = env['EXPECTED_FINGERPRINT']
    if not expected:
        raise ValueError('BLOCKED: 必须先从 init-keys 取得并填写 EXPECTED_FINGERPRINT')
    timeout = int(env['SMOKE_TIMEOUT_SECS'])
    base = 'https://' + env['AUTHORITY_DOMAIN']
    status, info = json_request(base + '/api/account/info', timeout)
    if status != 200 or info.get('mode') != 'authority':
        raise ValueError('HTTPS 权威模式探测失败')
    status, key = json_request(base + '/api/account/pubkey', timeout)
    if status != 200:
        raise ValueError('无法取得权威公钥')
    print('HTTPS / 权威模式 / 完整指纹：PASS ' + fingerprint(key, expected))
    if env['SMOKE_USERNAME']:
        body = {'username': env['SMOKE_USERNAME'], 'password': env['SMOKE_PASSWORD']}
        status, login = json_request(base + '/api/account/login', timeout, body)
        if status == 400 and env['SMOKE_SIGNUP'] == '1':
            signup_status, signup = json_request(base + '/api/account/signup', timeout, body)
            if signup_status != 200 or not signup.get('assertion', '').startswith('PA1.'):
                raise ValueError(f'测试账号注册失败 (HTTP {signup_status})')
            print('测试账号注册：PASS（产生持久测试账号，无邮箱绑定）')
            status, login = json_request(base + '/api/account/login', timeout, body)
        if status != 200 or not login.get('assertion', '').startswith('PA1.'):
            raise ValueError(f'测试账号登录失败 (HTTP {status})')
        print('测试账号登录：PASS；已存在账号时不重复注册')
    else:
        print('SKIP 注册/登录写操作：未配置专用测试账号；SMTP 发信未验证')
    if not authority_only:
        status, _ = request('https://' + env['RELAY_DOMAIN'] + '/ping', timeout)
        if status != 200:
            raise ValueError(f'relay HTTPS/TCP 探测失败 (HTTP {status})')
        print('relay HTTPS/TCP 443 / 证书验证：PASS')
        quic_probe(env['RELAY_DOMAIN'], timeout)
        print('relay UDP 7824 / QUIC 版本协商响应：PASS（不代表完整 QAD/中继往返测试）')


if __name__ == '__main__':
    try:
        if sys.argv[1] == 'fingerprint':
            print(fingerprint(json.load(sys.stdin), sys.argv[2] if len(sys.argv) > 2 else ''))
        elif sys.argv[1] == 'smoke':
            smoke(sys.argv[2], '--authority-only' in sys.argv[3:])
        else:
            raise ValueError('未知动作')
    except (ValueError, OSError, KeyError, urllib.error.URLError) as error:
        sys.exit(str(error))
