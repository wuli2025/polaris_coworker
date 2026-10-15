import type { EnvReport, VerifyReport } from '../../tauri';
import type { AgentBackendStatus } from '../../types/agentRuntime';
import { runtimeCopy as copy } from './copy';

export function legacyProviderActions(status: AgentBackendStatus | null): boolean {
  return status?.backend === 'claude-cli';
}
export function legacyDoctorActions(report: EnvReport | null): boolean {
  return report?.agentRuntime?.backend === 'claude-cli' && report.agentRuntime.legacyActions === true;
}
export function runtimeRows(report: EnvReport, verify: VerifyReport | null = null) {
  const r = report.agentRuntime;
  const flag = (value: boolean) => value ? copy.ready : copy.notReady;
  return [
    { label: 'Node', value: flag(r.nodeReady), detail: [report.node.version, report.node.path].filter(Boolean).join(' · ') },
    { label: 'agentd 入口', value: flag(r.entryExists) },
    { label: 'ping', value: flag(r.handshake) },
    { label: '协议版本', value: String(r.protocolVersion ?? copy.unknown) },
    { label: 'Pi 版本', value: r.piVersion ?? copy.unknown },
    { label: 'agentd 版本', value: r.agentdVersion ?? copy.unknown },
    { label: 'shell', value: flag(report.shellReady),
      detail: verify?.steps.find(s => s.key === 'shell')?.detail ?? copy.unknown },
    { label: '凭据', value: r.credentialsConfigured ? copy.configured : copy.unconfigured },
  ];
}
/** Current summaries omit price completeness. Never infer a total from a partial sum. */
export function usageCost(cost: number, pricesComplete = false): string {
  return pricesComplete && Number.isFinite(cost) && cost >= 0 ? `$${cost.toFixed(4)}` : copy.unknown;
}
export function recordedCost(cost: number): string {
  return Number.isFinite(cost) && cost > 0 ? `$${cost.toFixed(4)}` : copy.unknown;
}
