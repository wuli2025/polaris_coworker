//! Gateway endpoint construction and deferred relay updates.
use super::*;

pub(super) async fn ensure_endpoint() -> Result<std::sync::Arc<crate::collab::tunnel::relay::ConfiguredEndpoint>, String> {
    let mut g = ENDPOINT.lock().await;
    let relay_config = crate::collab::tunnel::relay::desired()?;
    if let Some(ep) = g.as_ref() {
        let has_live_connections = CONNS.read().values().any(|conn| conn.close_reason().is_none());
        if ep.uses_policy(&relay_config) || has_live_connections {
            return Ok(ep.clone());
        }
        ep.close().await;
        *g = None;
    }
    let seed = get_or_create_gateway_key()?;
    let transport = QuicTransportConfig::builder()
        .keep_alive_interval(KEEP_ALIVE)
        .max_idle_timeout(Some(
            IdleTimeout::try_from(IDLE_TIMEOUT).map_err(|e| format!("网关 idle timeout 无效: {e}"))?,
        ))
        .build();
    let ep = Endpoint::builder(presets::N0)
        .relay_mode(crate::collab::tunnel::relay::mode(&relay_config)?)
        .secret_key(SecretKey::from_bytes(&seed))
        .transport_config(transport)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| format!("网关 iroh endpoint 绑定失败: {e}"))?;
    let ep = crate::collab::tunnel::relay::track(ep, relay_config, "gateway");
    *g = Some(ep.clone());
    eprintln!("[gateway] iroh Endpoint 就绪 node_id={}", ep.id());
    Ok(ep)
}

