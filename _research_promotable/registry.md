# Source Registry (verified)

## GitHub stars (via api.github.com, June 2026)
- frp (fatedier/frp): 107,489
- rathole (rathole-org/rathole): 13,801
- bore (ekzhang/bore): 11,258
- rustdesk-server: 9,970
- Keycloak: 35,024
- authentik: 22,111
- Authelia: 28,113
- Zitadel: 14,147
- Casdoor: 13,812
- Logto: 12,182
- Ory Kratos: 13,711
- supabase/auth (GoTrue fork): ~2,467 (repo split; original netlify/gotrue archived)

## Architecture
- RustDesk: hbbs (ID/rendezvous, TCP 21115/16/18, UDP 21116) + hbbr (relay TCP 21117/19); ed25519 keypair; clients ping hbbs continuously; direct hole-punch first, hbbr fallback. Sources: rustdesk.com/docs, deepwiki.
- Tailscale: coordination server (control plane, key/metadata exchange only, no data) + DERP relays (HTTPS/443, WireGuard E2E, can't decrypt) + peer relays; >90% direct P2P. Sources: tailscale.com/blog/how-tailscale-works, docs.

## Tunnels
- Cloudflare Tunnel: free tunnel layer, HTTP-only, 100MB body limit, needs owned domain for persistent subdomain; Access $7/user >50 users. ngrok: not self-hostable (except enterprise), $14/extra domain/mo, 1GB free cap.

## Rust crypto
- argon2 (RustCrypto): Argon2id v19 default = OWASP min (19MiB/t=2/p=1). password-auth higher-level wrapper.
- jsonwebtoken (Keats) v11: auto-validates exp; pluggable aws_lc_rs / rust_crypto backend.
- OWASP order: Argon2id > scrypt > bcrypt(work>=10,<=72B) > PBKDF2(600k, FIPS).

## China compliance
- ICP备案: mandatory if server in mainland; overseas/HK/TW/Macau servers exempt. ~20-30 workdays.
- 公安备案: within 30 days of ICP via beian.gov.cn; also required if overseas-hosted but accessible from mainland.
- 工信部App备案 (信管〔2023〕105号): stock-app deadline 2024-03-31; new apps must file before launch; via 接入商/分发平台; 20 workday approval; display 备案号.
- 经营性ICP许可证 (增值电信-信息服务): required if charging/monetizing (会员/付费/广告变现); 注册资本 100万 (省内) / 1000万 (跨省); 5yr validity; 60 workday approval; often requires 等保2.0 二级+ as prerequisite.
- 软著: software copyright, needed for app-store distribution.
- 等保: 二级 (self-managed, biennial test) vs 三级 (public security oversight, annual test, ~8-12 security device classes, mandatory for large-volume personal info / transactions / social). Internet apps with mass users → 三级.
- PIPL: email = personal info; account password/financial = sensitive personal info. Requires: 单独同意 for sensitive/cross-border, privacy policy, minimal collection, encrypted storage, PIPIA for sensitive, breach notification (art.57), data localization preference. Fines up to 50M RMB or 5% annual revenue.
