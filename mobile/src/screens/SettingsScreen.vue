<script setup lang="ts">
import { computed } from "vue";
import { base, user, isOwner, displayName, logout } from "../lib/auth";
import { toggleableCaps, hasCap, setCap, type CapKey } from "../lib/capabilities";

// hasCap/toggleableCaps 内部读响应式 user+capOverride,computed 自动联动
const caps = computed(() => toggleableCaps());

function toggle(key: CapKey) {
  setCap(key, !hasCap(key));
}
function isOn(key: CapKey) {
  return hasCap(key);
}
</script>

<template>
  <div class="settings">
    <header class="bar"><div class="title">我的</div></header>
    <div class="scroll">
      <div class="profile">
        <div class="avatar">{{ displayName(user).slice(0, 1) }}</div>
        <div class="pmeta">
          <div class="pname">{{ displayName(user) }}</div>
          <div class="faint">
            {{ isOwner ? "管理员 · owner" : "成员 · member" }}
          </div>
        </div>
      </div>

      <div class="group">
        <div class="glabel">连接</div>
        <div class="krow">
          <span class="muted">中控主机</span>
          <span class="kval">{{ base || "—" }}</span>
        </div>
      </div>

      <div class="group">
        <div class="glabel">能力管控</div>
        <p class="faint gp">关闭的能力会从底部导航与「更多」面板隐藏。核心对话不可关闭。</p>
        <div v-for="c in caps" :key="c.key" class="caprow" @click="toggle(c.key)">
          <span class="cic">{{ c.icon }}</span>
          <span class="cmeta">
            <span class="clabel">{{ c.label }}</span>
            <span class="faint">{{ c.desc }}</span>
          </span>
          <span class="switch" :class="{ on: isOn(c.key) }"><i></i></span>
        </div>
      </div>

      <button class="btn ghost logout" @click="logout">退出登录</button>
      <p class="faint ver">北极星 · 安卓远程壳 v1.0</p>
    </div>
  </div>
</template>

<style scoped>
.settings {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.bar {
  padding: calc(10px + var(--safe-top)) 16px 10px;
  border-bottom: 1px solid var(--line);
}
.title {
  font-weight: 600;
}
.scroll {
  flex: 1;
  overflow-y: auto;
  padding: 16px 14px 30px;
}
.profile {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 6px 4px 18px;
}
.avatar {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--accent);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 22px;
  font-weight: 600;
}
.pname {
  font-size: 18px;
  font-weight: 600;
}
.group {
  background: var(--bg-elev);
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 14px;
  margin-bottom: 14px;
}
.glabel {
  font-size: 13px;
  color: var(--text-dim);
  margin-bottom: 8px;
}
.gp {
  margin: 0 0 10px;
}
.krow {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.kval {
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 60%;
}
.caprow {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 11px 0;
  border-top: 1px solid var(--line);
}
.cic {
  font-size: 22px;
}
.cmeta {
  flex: 1;
  display: flex;
  flex-direction: column;
}
.clabel {
  font-weight: 500;
}
.switch {
  width: 44px;
  height: 26px;
  border-radius: 13px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  position: relative;
  transition: background 0.15s;
  flex-shrink: 0;
}
.switch i {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--text-faint);
  transition: transform 0.15s, background 0.15s;
}
.switch.on {
  background: var(--accent);
  border-color: var(--accent);
}
.switch.on i {
  transform: translateX(18px);
  background: #fff;
}
.logout {
  width: 100%;
  margin-top: 6px;
  color: var(--danger);
}
.ver {
  text-align: center;
  margin-top: 18px;
}
</style>
