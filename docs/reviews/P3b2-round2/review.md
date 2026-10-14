

# Pi 独立审查报告

**审查模式**：packet（无工具）

---

## 结论

**CHANGES_REQUIRED**

C2 核心修复方向正确，但存在一个中等程度的逻辑缺口和测试覆盖不足；其他项（C3/#2/#5/#7/#9）修复合理。

---

## 范围与缺失材料

| 项目 | 内容 |
|------|------|
| 审查范围 | `R4-diff.patch` 全部 6 个文件 |
| 关键缺失 | `Config::protected()` 函数完整实现未提供；`validate_source` 双向前缀判定逻辑需反推 |
| 测试证据 | 仅 diff 内含测试代码，无沙箱运行报告 |

---

## 问题

### 1. 【中】受保护路径祖先漏检风险

**文件**：`src-tauri/crates/polaris-sandbox/src/policy.rs` 第 175-182 行

**证据**：
```rust
fn validate_mount_source(source: &Path, protected: &[PathBuf], home: &Path) -> Result<PathBuf> {
    let source = validate_source(source, protected)?;
    ensure!(!normalized(home)?.starts_with(&source),
        "SANDBOX_UNSAFE_MOUNT: home or ancestor; {REBUILD_HINT}");
    Ok(source)
}
```

**分析**：此函数只检查 `home` 是否以 `source` 为前缀（即 source 是否是 home 的祖先），但**没有检查 source 是否是受保护路径的祖先**。

- 受保护路径 `/home/user/.ssh`，项目 `/home/user`：双向前缀判定都返回 `false`（`/home/user` 不以 `/home/user/.ssh` 开头，反之亦然），项目可挂载。
- 按 C2 brief"项目包含受保护路径时拒绝"的描述，此场景应被拒绝，但实际可通过。

**影响**：用户可将包含受保护子目录的父目录挂载，容器内可见受保护内容。

**最小修复**：在 `validate_mount_source` 的 `ensure!` 前增加：
```rust
ensure!(!protected.iter().any(|p| source.starts_with(p)),
    "SANDBOX_UNSAFE_MOUNT: source is ancestor of protected path; {REBUILD_HINT}");
```

---

### 2. 【中】测试未验证"受保护"子目录实际可挂载

**文件**：`src-tauri/crates/polaris-sandbox/src/lifecycle.rs` 第 88-124 行

**证据**：
```rust
for suffix in ["src/data", "sessions", "fixtures"] {
    std::fs::create_dir_all(f.project.join(suffix))?;  // 创建目录
}
std::fs::write(f.project.join("fixtures/x.key"), "fixture")?;  // 创建文件
// ... 但之后只检查 Docker call 参数，没有验证容器内能否实际访问这些路径
```

**分析**：测试只断言：
1. Docker 调用参数正确（`--mount src=...,dst=/workspace`）
2. 受保护路径不触发 Docker 调用

**缺失**：未验证容器启动后，目录 `src/data`、`sessions`、文件 `fixtures/x.key` 是否真实存在于 `/workspace` 中。C2 核心目标"含这些目录的普通项目可以挂载"无直接证据。

**最小修复**：在 `start_with` 回调中，当检测到 `run` 命令时，解析 `--mount` 参数并验证挂载源确实包含上述目录，或添加注释说明通过检查挂载参数间接验证。

---

### 3. 【低】符号链接场景测试覆盖不完整

**文件**：同上测试函数

**证据**：测试未覆盖 brief 第 12 条要求的三类符号链接场景：
1. 项目本身是指向受保护目录的链接
2. 受保护目录是指向项目的链接
3. 两者共享祖先但通过链接跨越

**最小修复**：在测试中添加 `std::os::unix::fs::symlink` 创建符号链接场景，验证规范化后的路径判定正确。

---

## 建议验证步骤（区分推断与证据）

| 步骤 | 推断风险 | 已有证据 |
|------|---------|---------|
| 复现问题 1：项目 `/home/user` 含子目录 `/home/user/.ssh` 时挂载是否通过 | 高（逻辑缺口） | 无实测 |
| 验证符号链接规范化正确 | 中 | `normalized` 使用 `canonicalize` 逻辑合理，但未实测 |
| 确认 `protected()` 返回 HOME 及祖先 | 低 | C2 brief 描述与 `validate_mount_source` 的防御性检查一致 |
| 大小写不敏感文件系统 | 低 | 代码无特殊处理，`starts_with` 语义在 Windows 上行为需实测 |
| Windows 盘符与 UNC | 低 | `path_for_docker` 逻辑合理，vitest 未覆盖 |

---

## 无问题项确认

| 项 | 结论 |
|----|------|
| C3 偶发测试 | `fsyncSync` 后回复，消除时序依赖，逻辑正确 |
| #2 专家错误 | 错误含 `kind/id`，不泄露提示词，测试覆盖两种 loader 失败场景 |
| #5 前端路径提示 | 格式引导与后端校验分离，测试覆盖有效 |
| #9 注释 | `path_for_docker` 注释清晰说明 `\\?\` 前缀和斜杠处理 |

---

## 回归风险

- **低**：删除全树扫描后，每次路由性能应显著改善；保留双向前缀判定核心安全逻辑。
- **注意**：若 `protected()` 实现与 C2 brief 描述不符（如只返回固定列表而非动态计算 HOME 祖先），受保护路径覆盖可能不完整。
