//! Endpoint construction and reconnect lifetime.
use super::*;

/// 用种子建 Endpoint:N0 预设(打洞 + n0 DNS 发现 + 默认 relay)+ mDNS 内网发现,
/// 有自定义 relay 则覆盖 relay_mode。
pub(super) async fn build_endpoint(seed: [u8; 32], role: &'static str) -> Result<Arc<relay::ConfiguredEndpoint>, String> {
    // 保活 + 判死超时:没有它,NAT/relay 会静默回收空闲连接,直到下次请求才暴露断线。
    //
    // 流量窗口:QUIC 的默认值按「100ms RTT × 100Mbps」调,单流 1.25MB、连接级不设上限。
    // 两头都不合适 ——
    //  · 单流 1.25MB 在**高延迟长肥管**(比如跨境 200ms)上会把单流封在 6MB/s 左右,
    //    而这正是「拷一个大文件」的形态;放到 4MB 才够喂满。
    //  · 连接级不设上限意味着最坏情况是 `并发流数 × 单流窗口`,没有硬约束 —— 对
    //    1.6G 内存的小云机是个隐患。这里显式钉一个连接级总量(默认 32MB)作为**内存上限**,
    //    既拿到单流的速度,又不会因为对端猛开流把内存吃穿。
    // 两个都可用 env 覆盖(单位 KB):内存紧张的 NAS 调小,专线大带宽调大。
    let stream_rwnd = env_kb("POLARIS_QUIC_STREAM_WINDOW_KB", 4 * 1024).clamp(64 * 1024, 64 << 20);
    let conn_rwnd = env_kb("POLARIS_QUIC_CONN_WINDOW_KB", 32 * 1024).clamp(stream_rwnd, 512 << 20);
    let transport = QuicTransportConfig::builder()
        .keep_alive_interval(KEEP_ALIVE)
        .max_idle_timeout(Some(
            IdleTimeout::try_from(IDLE_TIMEOUT).map_err(|e| format!("idle timeout 无效: {e}"))?,
        ))
        .stream_receive_window(VarInt::from_u64(stream_rwnd).map_err(|e| format!("stream window 无效: {e}"))?)
        .receive_window(VarInt::from_u64(conn_rwnd).map_err(|e| format!("connection window 无效: {e}"))?)
        // 发送侧同样放到连接级上限,否则收端给了窗口发端也吐不出来。
        .send_window(conn_rwnd)
        .build();
    let mut builder = Endpoint::builder(presets::N0)
        .secret_key(SecretKey::from_bytes(&seed))
        .transport_config(transport)
        .alpns(vec![ALPN.to_vec()]);
    // 固定 UDP 端口(`POLARIS_IROH_PORT`)。默认是随机口,这对**跑在容器/NAT 后面的主机**
    // 是致命的:端口每次重启都变,docker 没法发布、防火墙没法放行 → 外面永远打不进来,
    // 打洞必失败、只能落中继(实测绕到 n0 德国中继,每跳往返 370ms)。钉死端口后
    // `-p 41641:41641/udp` + 云防火墙放行这一个口,成员端即可直连。0 = 保持随机。
    if let Some(p) = env_u16("POLARIS_IROH_PORT") {
        builder = builder
            .bind_addr((std::net::Ipv4Addr::UNSPECIFIED, p))
            .map_err(|e| format!("POLARIS_IROH_PORT={p} 绑定失败: {e}"))?;
    }
    let relay_config = relay::desired()?;
    builder = builder.relay_mode(relay::mode(&relay_config)?);
    let ep = builder
        .bind()
        .await
        .map_err(|e| format!("iroh endpoint 绑定失败: {e}"))?;
    // mDNS 局域网发现(iroh-mdns-address-lookup):纯内网(n0 DNS/relay 全不可达)时,
    // 同网段设备也能互相发现直连。尽力而为:失败只丢内网发现,不影响正常组网;
    // POLARIS_MDNS=0 可关(某些企业网禁组播,mDNS 只会白噪音)。
    let mdns_off = std::env::var("POLARIS_MDNS")
        .map(|v| v == "0")
        .unwrap_or(false);
    if !mdns_off {
        match iroh_mdns_address_lookup::MdnsAddressLookup::builder().build(ep.id()) {
            Ok(mdns) => match ep.address_lookup() {
                Ok(services) => services.add(mdns),
                Err(e) => eprintln!("[tunnel] mDNS 挂载失败(忽略,仅失去纯内网发现): {e}"),
            },
            Err(e) => eprintln!("[tunnel] mDNS 启动失败(忽略,仅失去纯内网发现): {e}"),
        }
    }
    Ok(relay::track(ep, relay_config, role))
}

/// Shared only while a client or its watchdog owns it. After the last disconnect,
/// the next connection builds a fresh endpoint with the latest relay policy.
pub(super) async fn client_endpoint() -> Result<Arc<relay::ConfiguredEndpoint>, String> {
    let mut slot = CLIENT_EP.lock().await;
    if let Some(endpoint) = slot.upgrade() {
        return Ok(endpoint);
    }
    let seed = get_or_create_device_key()?;
    let endpoint = build_endpoint(seed, "client").await?;
    *DEVICE_NODE.lock().map_err(|e| e.to_string())? = endpoint.id().to_string();
    *slot = Arc::downgrade(&endpoint);
    Ok(endpoint)
}

