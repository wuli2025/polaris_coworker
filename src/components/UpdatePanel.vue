<script setup lang="ts">
// 「更新」板块：显示当前版本、手动检查更新、一键更新。
// 与中央对话框(UpdateBanner)共享 useUpdater 的状态——启动自动检测，
// 这里则给用户一个随时主动检查的入口。
import { onMounted, computed } from "vue";
import {
  RefreshCw,
  Sparkles,
  CheckCircle2,
  Rocket,
  Container,
  ShieldAlert,
} from "@lucide/vue";
import OrbitSpinner from "./icons/OrbitSpinner.vue";
import {
  currentVersion,
  updateVersion,
  updateNotes,
  updating,
  updateProgress,
  updateError,
  checking,
  upToDate,
  checkFailed,
  lastCheckedAt,
  manualCheck,
  applyUpdate,
  loadUpdaterVersion,
  updaterRuntime,
  dockerUpdaterEnabled,
  dockerUpdaterServiceConfigured,
  dockerUpdateScriptPresent,
  dockerMessage,
} from "../composables/useUpdater";

onMounted(() => {
  void loadUpdaterVersion();
});

const isDocker = computed(() => updaterRuntime.value === "docker");

const lastChecked = computed(() => {
  if (!lastCheckedAt.value) return "";
  const d = new Date(lastCheckedAt.value);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}`;
});
</script>

<template>
  <div class="up-panel">
    <header class="up-header">
      <h1>更新</h1>
      <p class="up-sub">保持 Polaris 为最新版本</p>
    </header>

    <div class="up-body">
      <!-- 当前版本 -->
      <div class="ver-card">
        <img class="ver-logo" src="../assets/logo.png" alt="北极星" />
        <div class="ver-meta">
          <div class="ver-name">北极星 · Polaris</div>
          <div class="ver-num">当前版本 v{{ currentVersion || "—" }}</div>
          <span v-if="isDocker" class="runtime-badge"><Container :size="11" /> Docker 远程版</span>
        </div>
        <button
          class="ck-btn"
          :disabled="checking || updating"
          @click="manualCheck"
        >
          <OrbitSpinner
            v-if="checking"
            :size="15"
          />
          <RefreshCw v-else :size="15" :stroke-width="2" />
          <span>{{ checking ? "检查中…" : "检查更新" }}</span>
        </button>
      </div>

      <!-- 状态 / 更新区 -->
      <div class="state">
        <!-- 发现新版本 -->
        <div v-if="updateVersion" class="found">
          <div class="found-top">
            <span class="found-badge"><Sparkles :size="18" :stroke-width="1.7" /></span>
            <div>
              <div class="found-title">
                发现新版本 <b>v{{ updateVersion }}</b>
              </div>
              <div class="found-hint">
                <template v-if="isDocker">
                  {{
                    updating
                      ? dockerMessage || "隔离更新服务已接单，容器即将短暂断线"
                      : dockerUpdaterEnabled
                        ? "点「立即更新」后由隔离服务拉取镜像并安全替换当前容器"
                        : "已检测到新版；启用下方安全更新配置后可在这里一键更新"
                  }}
                </template>
                <template v-else>
                  {{ updating ? "正在下载，完成后自动重启生效" : "点「立即更新」后台下载安装，自动重启即用" }}
                </template>
              </div>
            </div>
          </div>

          <div v-if="updateNotes && !updating" class="found-notes">{{ updateNotes }}</div>

          <div v-if="updating" class="bar">
            <div class="bar-fill" :style="{ width: updateProgress + '%' }"></div>
          </div>

          <button
            class="go-btn"
            :disabled="updating || (isDocker && !dockerUpdaterEnabled)"
            @click="applyUpdate()"
          >
            <OrbitSpinner
              v-if="updating"
              :size="15"
            />
            <Rocket v-else :size="15" :stroke-width="1.9" />
            <span v-if="isDocker">
              {{ updating ? "正在交接容器更新…" : dockerUpdaterEnabled ? "立即更新容器" : "需先启用安全更新" }}
            </span>
            <span v-else>{{ updating ? `更新中 ${updateProgress}%` : "立即更新" }}</span>
          </button>
        </div>

        <!-- 已是最新 -->
        <div v-else-if="upToDate" class="ok">
          <CheckCircle2 :size="18" :stroke-width="1.8" />
          <span>已是最新版本</span>
        </div>

        <!-- 自动检查失败（非静默，引导用户手动检查） -->
        <div v-else-if="checkFailed && !updateVersion" class="err">
          <div>自动检查更新失败: {{ updateError || "网络或服务端异常" }}</div>
          <div style="margin-top:4px;font-size:11px;color:var(--dim)">
            可点击上方「检查更新」重试，或前往
            <a href="https://github.com/wuli2025/polaris_coworker/releases" target="_blank" style="color:var(--primary)">GitHub Releases</a>
            手动下载
          </div>
        </div>

        <!-- 错误 -->
        <div v-else-if="updateError" class="err">{{ updateError }}</div>

        <!-- 空闲 -->
        <div v-else class="idle">
          {{ isDocker ? "容器启动后会自动检查官方镜像版本" : "Polaris 启动时会自动检查更新" }}
        </div>

        <div v-if="lastChecked" class="last">上次检查 {{ lastChecked }}</div>
      </div>

      <!-- 旧镜像先迁移一次；新架构由隔离 updater 提供页面更新。 -->
      <div v-if="isDocker && !dockerUpdaterEnabled" class="docker-setup">
        <div class="docker-setup-head">
          <ShieldAlert :size="18" :stroke-width="1.8" />
          <div>
            <div class="docker-setup-title">一键更新尚未启用</div>
            <div class="docker-setup-sub">
              <template v-if="!dockerUpdateScriptPresent">
                当前是旧版或自建镜像，镜像内没有更新脚本；请先执行官网的一次迁移命令。
              </template>
              <template v-else-if="!dockerUpdaterServiceConfigured">
                内部更新服务尚未启动；请重新运行官网安装/迁移命令。
              </template>
              <template v-else>
                更新服务尚未就绪，请重新运行官网安装/迁移命令。
              </template>
            </div>
          </div>
        </div>
        <code class="docker-command">docker compose -f docker-compose.yml -f docker-compose.update.yml up -d</code>
        <p class="docker-warning">
          Docker socket 只挂在固定版本的 Watchtower sidecar，绝不进入 Polaris App 容器。
          App 与 sidecar 的通信密钥由部署文件在容器内部处理，无需用户配置。
        </p>
      </div>

      <!-- 工作原理 -->
      <div class="how">
        <div class="how-title">更新是怎么工作的</div>
        <ol v-if="isDocker">
          <li>启动时读取当前架构镜像的 OCI build revision，确认运行中的容器是否落后</li>
          <li>点「立即更新容器」后，请求会立即交给隔离 Watchtower；页面按 requestId 跟踪，不依赖旧容器保持连接</li>
          <li>只有新 boot 与目标 revision 同时命中、且服务重新就绪才算成功；失败或 15 分钟未确认会停止等待并显示重试指引</li>
        </ol>
        <ol v-else>
          <li>启动时自动检查 GitHub 上有没有新版本</li>
          <li>发现新版会在屏幕中央弹一个轻提示，点「立即更新」即可</li>
          <li>后台静默下载并安装，<b>自动重启</b>到新版 —— 无需手动重装</li>
        </ol>
      </div>
    </div>
  </div>
</template>

<style scoped>
.up-panel {
  height: 100%;
  overflow-y: auto;
  background: var(--bg);
  padding: 28px 32px 40px;
}
.up-header {
  margin-bottom: 22px;
}
.up-header h1 {
  margin: 0;
  font-family: var(--serif);
  font-size: 22px;
  font-weight: 600;
  color: var(--ink);
  letter-spacing: 2px;
}
.up-sub {
  margin: 4px 0 0;
  font-size: 12.5px;
  color: var(--muted);
}
.up-body {
  max-width: 560px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.ver-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 16px 18px;
  background: var(--panel);
  border: 1px solid var(--border-soft);
  border-radius: 14px;
}
.ver-logo {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  object-fit: contain;
  flex-shrink: 0;
}
.ver-meta {
  flex: 1;
  min-width: 0;
}
.ver-name {
  font-family: var(--serif);
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
  letter-spacing: 1px;
}
.ver-num {
  margin-top: 2px;
  font-size: 12px;
  color: var(--muted);
}
.runtime-badge {
  width: fit-content;
  margin-top: 6px;
  padding: 3px 7px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 1px solid color-mix(in srgb, var(--primary) 24%, var(--border));
  border-radius: 999px;
  background: var(--primary-soft);
  color: var(--primary);
  font-size: 10.5px;
  font-weight: 600;
}
.ck-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-soft);
  color: var(--text);
  font-size: 12.5px;
  font-weight: 500;
  flex-shrink: 0;
}
.ck-btn:hover:not(:disabled) {
  border-color: var(--primary);
  color: var(--primary);
}
.ck-btn:disabled {
  opacity: 0.65;
  cursor: default;
}

.state {
  padding: 4px 2px;
}
.found {
  padding: 16px;
  background: var(--primary-soft);
  border: 1px solid color-mix(in srgb, var(--primary) 28%, transparent);
  border-radius: 14px;
}
.found-top {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}
.found-badge {
  width: 34px;
  height: 34px;
  border-radius: 9px;
  background: var(--panel);
  color: var(--primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.found-title {
  font-size: 14px;
  color: var(--text);
  font-weight: 500;
}
.found-title b {
  color: var(--primary);
}
.found-hint {
  margin-top: 3px;
  font-size: 11.5px;
  color: var(--muted);
}
.found-notes {
  margin-top: 12px;
  max-height: 120px;
  overflow-y: auto;
  padding: 10px 12px;
  background: var(--panel);
  border-radius: 10px;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--text-2);
  white-space: pre-wrap;
}
.bar {
  margin-top: 14px;
  height: 6px;
  border-radius: 3px;
  background: var(--panel);
  overflow: hidden;
}
.bar-fill {
  height: 100%;
  background: var(--primary);
  border-radius: 3px;
  transition: width 0.2s ease;
}
.go-btn {
  margin-top: 14px;
  width: 100%;
  padding: 11px 0;
  border: none;
  border-radius: 11px;
  background: var(--btn-solid-bg);
  color: var(--btn-solid-text);
  font-size: 13.5px;
  font-weight: 600;
  letter-spacing: 1px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
}
.go-btn:hover:not(:disabled) {
  background: var(--primary);
}
.go-btn:disabled {
  opacity: 0.85;
  cursor: default;
}
.ok {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--primary);
  font-weight: 500;
}
.err {
  font-size: 12.5px;
  color: var(--vermilion);
  line-height: 1.6;
}
.idle {
  font-size: 12.5px;
  color: var(--muted);
}
.last {
  margin-top: 8px;
  font-size: 11px;
  color: var(--dim);
}

.docker-setup {
  padding: 15px 17px;
  border: 1px solid color-mix(in srgb, var(--vermilion) 24%, var(--border));
  border-radius: 14px;
  background: color-mix(in srgb, var(--vermilion) 6%, var(--panel));
}
.docker-setup-head {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  color: var(--vermilion);
}
.docker-setup-head > svg {
  margin-top: 1px;
  flex-shrink: 0;
}
.docker-setup-title {
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
}
.docker-setup-sub {
  margin-top: 3px;
  color: var(--text-2);
  font-size: 11.5px;
  line-height: 1.6;
}
.docker-setup code {
  font-family: var(--mono);
}
.docker-command {
  display: block;
  margin-top: 12px;
  padding: 9px 11px;
  overflow-x: auto;
  border: 1px solid var(--border-soft);
  border-radius: 9px;
  background: var(--panel);
  color: var(--text);
  font-size: 10.5px;
  white-space: nowrap;
}
.docker-warning {
  margin: 10px 0 0;
  color: var(--muted);
  font-size: 10.5px;
  line-height: 1.65;
}
.docker-warning code,
.docker-setup-sub code {
  color: var(--text-2);
}

.how {
  margin-top: 4px;
  padding: 16px 18px;
  background: var(--bg-soft);
  border: 1px solid var(--border-soft);
  border-radius: 14px;
}
.how-title {
  font-family: var(--serif);
  font-size: 12.5px;
  letter-spacing: 1.5px;
  color: var(--text-2);
  margin-bottom: 8px;
}
.how ol {
  margin: 0;
  padding-left: 18px;
}
.how li {
  font-size: 12px;
  color: var(--muted);
  line-height: 1.9;
}
.how li b {
  color: var(--text-2);
}
.spin {
  animation: up-spin 0.9s linear infinite;
}
@keyframes up-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
