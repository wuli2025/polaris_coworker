/**
 * 手机端 iroh 隧道(路线 B:P2P 直连桌面)——Capacitor 原生插件封装。
 *
 * 原生实现 mobile/native/polaris-tunnel(iroh,镜像桌面 tunnel.rs 成员端),经
 * PolarisTunnel 插件暴露。只在安卓原生壳里可用;浏览器 dev / 无插件时降级为不可用,
 * 由调用方回落到云网关(中继)。
 *
 * 连接模型:startClient(hostNodeId, localPort) 在本机起 127.0.0.1:localPort ↔ 主机
 * 的 iroh 隧道,WebView 随后把 base 设成 http://127.0.0.1:localPort 即经隧道访问桌面。
 */
import { registerPlugin } from "@capacitor/core";

interface PolarisTunnelPlugin {
  deviceNodeId(): Promise<{ nodeId: string }>;
  startClient(opts: { hostNodeId: string; listenPort: number }): Promise<{ ok: boolean; error: string }>;
  stopClient(): Promise<void>;
  tunnelState(): Promise<{ running: boolean; state: string; last_error: string }>;
}

const Plugin = registerPlugin<PolarisTunnelPlugin>("PolarisTunnel");

/** 原生隧道是否可用(安卓壳 + .so 已装)。探测一次并缓存。 */
let available: boolean | null = null;
export async function tunnelAvailable(): Promise<boolean> {
  if (available !== null) return available;
  try {
    await Plugin.deviceNodeId();
    available = true;
  } catch {
    available = false;
  }
  return available;
}

/** 本机设备 NodeId(主机据此加白名单);不可用时返回 null。 */
export async function deviceNodeId(): Promise<string | null> {
  try {
    const r = await Plugin.deviceNodeId();
    return r.nodeId || null;
  } catch {
    return null;
  }
}

/** 本机固定的隧道回环端口(手机端一次只连一台主机,固定端口即可)。 */
export const TUNNEL_PORT = 18617;

/**
 * 起 P2P 隧道到主机 NodeId,成功返回本地 base(http://127.0.0.1:port),失败返回 null
 * (调用方回落云网关)。
 */
export async function startTunnel(hostNodeId: string): Promise<string | null> {
  try {
    const r = await Plugin.startClient({ hostNodeId, listenPort: TUNNEL_PORT });
    if (!r.ok) return null;
    return `http://127.0.0.1:${TUNNEL_PORT}`;
  } catch {
    return null;
  }
}

export async function stopTunnel(): Promise<void> {
  try {
    await Plugin.stopClient();
  } catch {
    /* ignore */
  }
}

export async function tunnelState(): Promise<{ running: boolean; state: string; last_error: string } | null> {
  try {
    return await Plugin.tunnelState();
  } catch {
    return null;
  }
}
