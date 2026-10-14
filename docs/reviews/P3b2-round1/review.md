## 审查结果

**审查模式：packet（无工具）** — 仅基于 brief、`P3b2-diff.patch` 内联材料及注释进行评审，未使用任何读写/编辑/执行工具。

**结论：CHANGES_REQUIRED**

---

## 范围与缺失材料

**已覆盖：**
- `polaris-sandbox` crate：挂载策略（`policy.rs`）、生命周期（`lifecycle.rs`）、配置（`config.rs`）、命令（`commands.rs`）
- `polaris-kernel` crate：`SandboxPolicy` trait、`ExpertLoader` trait、`expert_prompt` 传递链路
- 前端：`SandboxProject.vue`、`expertSelection.ts`、ChatComposer 传递逻辑
- brief 全部 5 项验收条件的实现范围

**未提供/不确定：**
- `polaris-sandbox/src/policy_tests.rs` 内容（diff 中声明存在但未内联）
- 前端 TypeScript 端 `ExpertSelection` 类型与 Rust 端 `ExpertSelection` enum 的 `kind` 字段序列化对应性（需实际测试）
- Windows 环境下的 `path_for_docker()` 行为（仅 `path_for_docker_normalizes_on_windows` 测试存在）
- 目录内容扫描在大目录下的性能表现

---

## 严重问题

### 1. `validate_container()` 缺少目录内容扫描，可绕过禁止文件检查
**文件：** `src-tauri/crates/polaris-sandbox/src/policy.rs`  
**行：** 784–809（`validate_container` 函数）

**证据：** `validate_source()`（724–746 行）执行递归目录扫描，检查 `forbidden_name`（`providers.json`、`.key`、`.db` 等）。但 `validate_container()` 只对每个 mount 调用 `validate_source(Path::new(source), ...)`，这只检查路径自身（父路径是否被保护、是否目录、路径组件名是否禁止），**不对挂载目录的内容做递归扫描**。

```rust
// 第 791 行：只检查 mount["Source"] 路径本身
validate_source(Path::new(source), &self.protected(home))?;
```

`validate_source()` 内部调用 `source.is_dir()` 和 `forbidden_name()` 组件检查，但递归扫描 `read_dir` 的内容检查（第 736–745 行）只在传入的 `source` 路径**直接包含**禁止文件时触发——不扫描已挂载目录的完整子树。

**可复现影响：** 用户先创建容器（此时 `~/Polaris/data/providers.json` 尚不存在），后生成明文 Key 文件，重新执行工具时容器挂载路径仍指向目标项目，`validate_source()` 检查路径组件名不包含 `data`，容器被判定为安全，Key 文件被暴露。

**最小修复建议：** 在 `validate_container()` 的 mounts 循环内，对每个挂载目录执行完整的 `read_dir` 递归扫描，复用 `validate_source()` 的目录内容检查逻辑（736–745 行），或直接对 mount source 调用包含完整扫描的辅助函数。

---

### 2. 专家请求级传递链路中诊断信息缺失
**文件：** `src-tauri/crates/polaris-kernel/src/chat/backend/host_tools.rs`  
**行：** 218（`expert_prompt_with`）

**证据：**
```rust
loader.ok_or_else(|| anyhow::anyhow!("EXPERT_LOADER_UNAVAILABLE"))?.load(selection)
```
错误消息仅为字符串字面量，不含请求上下文（expert id / team id / 来自哪个前端组件）。

**可复现影响：** Pi 后端下若专家加载器未注册，错误消息无法关联到具体请求，调试困难。

**最小修复建议：** 将错误消息改为包含 id 信息：
```rust
loader.ok_or_else(|| anyhow::anyhow!("EXPERT_LOADER_UNAVAILABLE for selection: {:?}", selection))?
```

---

## 中等问题

### 3. `forbidden_name()` 仅检查路径组件，忽略路径结构
**文件：** `src-tauri/crates/polaris-sandbox/src/policy.rs`  
**行：** 732

```rust
ensure!(!source.components().any(|p| forbidden_name(&p.as_os_str().to_string_lossy())),
    "SANDBOX_UNSAFE_MOUNT: forbidden path; {REBUILD_HINT}");
```

**证据：** 对 `/home/user/workspace/Polaris/data/keys` 检查组件中有 `data`（触发），但对 `/home/user/data/workspace`（`data` 为路径前缀）则不会触发——`data` 只匹配组件名而非相对路径。

**可复现影响：** 低——实际场景中用户项目目录名很少恰好叫 `data`；但 `providers.json`、`.key` 文件在子目录中可能被遗漏。

**最小修复建议：** 对完整相对路径（相对于 home）的每个路径层级都检查 `forbidden_name`，或对整个相对路径检查是否以禁止名称开头/结尾。

---

### 4. `workdir_change` 静默复用可能使用户不知情地在错误目录工作
**文件：** `src-tauri/crates/polaris-sandbox/src/lifecycle.rs`  
**行：** 621–629

```rust
Some(Err(error)) if rebuild || (config.workdir_change == WorkdirChange::Rebuild
    && error.to_string().starts_with("SANDBOX_PATH_UNMOUNTED:")) => Ok(Action::PreserveAndRebuild),
Some(Err(error)) => Err(error),
```

**证据：** 当 `rebuild=false` 且 `workdir_change=Reject` 时，任何验证错误都被直接返回（用户必须手动 rebuild）。但当 `workdir_change=Rebuild` 且错误为 `SANDBOX_PATH_UNMOUNTED` 时，旧容器被静默复用（`Action::Reuse`）——用户以为自己在新目录工作，实际仍使用旧挂载。

**可复现影响：** 中等——如果用户误输入拼写略有不同的项目路径，容器不会重建，`validate_container` 返回错误，用户收到 `SANDBOX_PATH_UNMOUNTED` 但容器仍以旧路径运行，用户可能未察觉。

**最小修复建议：** 在 `validate_container` 返回错误后、用户未选择 rebuild 之前，在 `sandbox_status` 或前端 `SandboxProject` 中显示"当前容器使用目录 X，与请求的 Y 不一致"的警告，不依赖静默复用。

---

### 5. 前端路径输入无即时校验
**文件：** `src/features/sandbox/components/SandboxProject.vue`  
**行：** 989–996

```typescript
const migration = computed(() => needsSandboxMigration([...(props.notes ?? []), error.value]));
// directory.value 为文本输入框，用户可直接输入任意字符串
```

**证据：** 用户可在 `input` 中输入非绝对路径、空字符串（虽然 `start()` 会检查），但后端返回错误前无前端反馈。

**可复现影响：** 低——后端有验证；用户体验略差（需等待后端响应才知道路径无效）。

**最小修复建议：** 前端对 `directory` 输入做基础格式检查（`startsWith('/')` 或 `isAbsolute()`），在 blur/change 时即时提示。

---

### 6. Docker `--mount` 参数中冒号未在源路径中拒绝
**文件：** `src-tauri/crates/polaris-sandbox/src/policy.rs`  
**行：** 776

```rust
ensure!(!source.contains([',', '\n', '\r']), "SANDBOX_INVALID_PATH: mount delimiter");
```

**证据：** `--mount type=bind,src=path,dst=dest` 使用逗号作为字段分隔符，`validate_source()` 正确拒绝含逗号和换行的路径。但冒号在 `--mount` 语法中作为键值对分隔符（`src=path:value`，`key:value`），含冒号的路径可能破坏 Docker 解析。

**可复现影响：** 中——含冒号的路径在 Docker 中会导致难以理解的参数解析错误，而非友好的验证错误。

**最小修复建议：** 在 `validate_source()` 中也对路径中的冒号进行检查：
```rust
ensure!(!source.contains(':'), "SANDBOX_INVALID_PATH: colon not allowed in mount source");
```

---

### 7. `mount_tests` 被替换，未同时保留直接挂载验证
**文件：** `src-tauri/crates/polaris-kernel/src/chat/backend/docker_tools.rs`  
**行：** 79–102（diff 显示被删除）

**证据：** 旧 `mount_tests` 直接验证挂载路由（`old_whole_app_mount_is_rejected_and_project_mount_is_routed`）。新测试 `policy_port_tests` 验证 `SandboxPolicy` trait 调用和拒绝传播。**两者测试层次不同**，旧测试覆盖端到端挂载选择逻辑，新测试覆盖 trait 接口。

**可复现影响：** 中——`SandboxPolicy` trait 的直接实现测试缺失；如果未来 `ShellSandboxPolicy`（`wiring.rs`）实现有 bug，`policy_port_tests` 可能无法捕获直接挂载场景的回归。

**最小修复建议：** 保留原 `mount_tests` 或将其迁移到 `polaris-sandbox` crate 作为集成测试，确保端到端挂载选择逻辑被覆盖。

---

## 轻微问题

### 8. `feishu.rs` 中 `expert_selection` 硬编码为 `None`
**文件：** `src-tauri/crates/polaris-kernel/src/integrations/feishu.rs`  
**行：** 261

飞书集成当前不支持专家选择，功能已明确，后续接入时需补全。

### 9. `path_for_docker` 缺少文档注释
**文件：** `src-tauri/crates/polaris-sandbox/src/policy.rs`  
**行：** 704

Windows 路径规范化和 `\\?\` 前缀处理逻辑无注释，后续维护可能不清楚。

---

## 建议验证步骤（区分推断风险与已有证据）

### 推断风险（无测试证据）
1. **含冒号路径在 Docker 中的实际行为**：需在 Docker 环境中测试 `/path:with:colons` 作为 `--mount src=` 参数时的解析结果
2. **`validate_container` 目录内容扫描缺失的绕过**：需构造"容器创建后新增禁止文件"的场景并验证当前行为
3. **Windows 大小写规范化**：`forbidden_name` 用 `to_ascii_lowercase()` 比较，Windows 路径大小写不敏感场景需实际验证
4. **TypeScript `kind: "expert"` 与 Rust `ExpertSelection::Expert` 序列化匹配**：确认 Rust 的 `rename_all = "kebab-case"` 对 `kind` 字段（`"expert"` vs `"Expert"`）的处理符合预期

### 已有证据（diff 和注释）
1. ✅ `policy_port_tests`：验证 `SandboxPolicy` trait 调用和拒绝传播（行 112–137）
2. ✅ `migration_retains_old_container_and_never_deletes_it`：验证旧容器通过 rename 保留、不执行 rm（行 661–688）
3. ✅ `explicit_selection_loads_exact_id_and_rejects_invalid_input`：验证专家选择加载和反序列化校验（行 165–187）
4. ✅ `path_for_docker_normalizes_on_windows`：验证 Windows 路径规范化（行 488–495）
5. ✅ `every_route_passes_full_inspection_to_policy_and_propagates_rejection`：验证策略检查在路由时被调用两次（行 127–136）

---

## 总结

本次审查发现 **1 个严重阻塞问题**（`validate_container()` 缺少目录内容扫描，导致容器运行期间新增的禁止文件可被绕过）和 **6 个中等严重程度问题**。最关键的是容器运行时验证必须对挂载目录的完整内容执行递归扫描，以防止旧容器在敏感文件生成后绕过安全检查。`SandboxPolicy` trait 设计总体合理，专家传递链路清晰，`Config::protected()` 的基线路径硬编码保证用户配置无法移除核心保护。测试覆盖基本到位，但 `validate_container` 的内容扫描缺陷是必须修复的安全漏洞。
