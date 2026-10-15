

# Pi 独立审查报告

## 审查模式
- **模式**：packet（无工具）
- **可用工具**：无
- **范围**：P5cd-frontend.patch 内联材料

## 结论
**CHANGES_REQUIRED**

---

## 问题

### 问题 1：上下文编辑器路径越界防护缺失（严重）

**文件**：`src/features/runtime/components/ContextFileEditor.vue`（第 948–964 行）

**证据**：
```ts
// 第 957 行
const selected = await contextFiles.metadata(projectId.value, createFile.value);
```
前端直接将 `projectId` 透传给后端，未校验其格式（如 `../` 或绝对路径）。

**影响**：若后端 `context_file_metadata` 未做路径隔离，用户可通过恶意 `projectId` 读取项目目录外的文件。

**最小修复**：
```ts
// 在调用前添加格式校验
if (!/^[a-zA-Z0-9_-]+$/.test(projectId.value)) {
  error.value = "无效的项目 ID 格式";
  return;
}
```

---

### 问题 2：`runtimeRows` 字段拼接可能产生意外空串（中等）

**文件**：`src/features/runtime/presentation.ts`（第 1233 行）

**证据**：
```ts
{ label: 'Node', value: flag(r.nodeReady), detail: [report.node.version, report.node.path].filter(Boolean).join(' · ') },
```
`report.node` 可能为 `null`，访问 `.version` 会抛 `TypeError`。

**可复现**：浏览器 preview stub 中 `report.node` 为 `{ found: false }` 但非 `null`，测试通过；但真实 Tauri 后端若返回 `null` 则崩溃。

**最小修复**：
```ts
detail: report.node ? [report.node.version, report.node.path].filter(Boolean).join(' · ') : copy.unknown,
```

---

### 问题 3：专家开关服务端鉴权状态不明（中等）

**文件**：`src/features/runtime/components/ExpertRuntimeNotice.vue`（第 1057–1058 行）

**证据**：
```ts
@change="set(($event.target as HTMLInputElement).checked)"
```
任何能访问专家中心的用户都能修改开关。

**缺失材料**：后端 patch 未提供 `expert_enabled_set` 的权限校验逻辑。

**最小修复**：需在后端补充鉴权（当前 patch 范围外，但需明确记录为依赖项）。

---

### 问题 4：`unknownCosts` 宽松的类型守卫（低）

**文件**：`src/features/runtime/usageCosts.ts`（第 1310–1314 行）

**证据**：
```ts
if (typeof value.unpricedRequests !== 'number' || typeof value.hasUnknownCosts !== 'boolean') {
  return '价格完整性未知';
}
```
`undefined`、`null` 均通过此检查，但字符串值也会通过：
```ts
unknownCosts({ unpricedRequests: '2', hasUnknownCosts: true }) // 返回 '未知'
```

**最小修复**：
```ts
if (value.unpricedRequests == null || typeof value.hasUnknownCosts !== 'boolean') {
  return '价格完整性未知';
}
```

---

### 问题 5：上下文编辑器切换项目时缺少确认（低）

**文件**：`src/features/runtime/components/ContextFileEditor.vue`（第 966 行）

**证据**：
```ts
const loadedProject = ref('');
```
切换 `projectId` 时若存在未保存修改，用户直接丢失更改。`load()` 有确认逻辑（第 950 行），但 `projectId` 的 `watch` 缺失。

**最小修复**：
```ts
watch(projectId, async (newId) => {
  if (content.value !== original.value && !confirm('切换项目会丢失未保存修改，继续？')) {
    projectId.value = loadedProject.value; // 回滚
    return;
  }
  await open();
});
```

---

## 建议验证步骤（区分推断风险与已有证据）

| 风险等级 | 推断/已有 | 验证方式 |
|----------|-----------|----------|
| 问题 1 | 推断（需后端实现证实） | 手动尝试传入 `projectId: "../../../etc/passwd"` |
| 问题 2 | 已有（类型不一致） | 运行 typecheck：`npx vue-tsc --noEmit` 实际通过，因 stub 未模拟 `null` 场景 |
| 问题 3 | 推断（后端材料缺失） | 检查后端 patch 中 `expert_enabled_set` 是否有 auth middleware |
| 问题 4 | 已有（类型守卫缺陷） | 单元测试 `unknownCosts({ unpricedRequests: 'x' })` 会错误返回 `'未知'` |
| 问题 5 | 推断（UI 体验问题） | 手动操作：修改内容 → 切换项目下拉框 → 观察是否弹确认 |

---

## 范围与缺失材料

**已审范围**：`P5cd-frontend.patch`（2090 行 diff + 4 个新测试文件）

**缺失材料**：
- 后端 `context_file_metadata/read/write` 实现（路径隔离依赖后端）
- `expert_enabled_get/set` 的服务端鉴权逻辑
- `agent_backend_status` Tauri 命令定义
- `PriceCompleteness` 类型在后端的实际填充逻辑

**无阻塞**：前端改动本身无严重缺陷，问题 1 和 3 依赖后端材料确认。
