

## 审查结果

**审查模式**：packet（无工具）

---

### 结论：**CHANGES_REQUIRED**

---

### 范围与缺失材料

- **已审**：diff patch（含测试）、round1 triage、brief
- **缺失**：未提供 Rust 源码全文、未提供 `pi_shell.rs` 测试文件的 `#[cfg(windows)]` 条件编译上下文、未提供 `which_all` 实现
- **无法验证**：Linux 非 Windows 环境下 `ShellSource::Bundled`/`ProgramFiles` 变体是否导致测试编译错误（见下方问题 1）

---

### 问题

#### 问题 1（严重）：`pi_tests.rs` 中跨平台枚举字面量可能违反 Rust 编译安全

**文件**：`src-tauri/crates/polaris-kernel/src/doctor/pi_tests.rs:318-319`

**证据**：
```rust
for (source, label) in [(ShellSource::Bundled, "bundled"), (ShellSource::ProgramFiles, "program_files"),
    (ShellSource::Path, "path"), (ShellSource::Explicit, "explicit")] {
```

**问题**：`ShellSource` 枚举定义（`pi_shell.rs:103-111`）对 `Bundled` 和 `ProgramFiles` 变体标注了 `#[cfg(any(windows, test))]`，而 `pi_tests.rs` 的模块路径（`pi.rs:93`）为 `#[path = "pi_tests.rs"]`——这意味着该文件在所有平台均被编译。Rust 不允许引用条件编译不存在的枚举变体，即使在 `cfg(windows)` 测试块内也不行。

**可复现影响**：在 Linux 上 `cargo check`/`cargo build` 将失败，brief 中所述的"沙箱外两壳 cargo check 0 警告"与 Linux 行为矛盾。

**最小修复**：
```rust
// 添加 #[cfg(windows)] 条件或改用 cfg_attr
#[cfg(windows)]
const WINDOWS_SOURCES: &[(ShellSource, &str)] = &[
    (ShellSource::Bundled, "bundled"),
    (ShellSource::ProgramFiles, "program_files"),
    (ShellSource::Path, "path"),
    (ShellSource::Explicit, "explicit"),
];
#[cfg(not(windows))]
const WINDOWS_SOURCES: &[(ShellSource, &str)] = &[
    (ShellSource::Path, "path"),
    (ShellSource::Explicit, "explicit"),
];
for (source, label) in WINDOWS_SOURCES { ... }
```

---

#### 问题 2（严重）：Linux `resolve()` 返回值类型与 `ResolvedShell` 定义不一致

**文件**：`src-tauri/crates/polaris-kernel/src/doctor/pi_shell.rs:197-202`

**证据**：
```rust
#[cfg(not(windows))]
{ Ok(if Path::new("/bin/bash").is_file() { Some("/bin/bash".into()) }  // ← PathBuf
    else { super::probe::which_all("bash").into_iter().next()
        .or_else(|| super::probe::which_all("sh").into_iter().next()) })  // ← PathBuf
    .map(|path| ResolvedShell { path, source: ShellSource::Path })) }  // ← 转换为 ResolvedShell
```

**问题**：
1. 非 Windows 分支返回 `Result<Option<ResolvedShell>, ...>` 与函数签名一致（`pub(crate) fn resolve`），但 `ShellSource::Bundled` 和 `ShellSource::ProgramFiles` 在 Linux 不可用——这两个变体是 `#[cfg(any(windows, test))]`
2. 代码逻辑本身无错误，但 `ShellSource` 枚举在 Linux 上只有 `Path` 和 `Explicit` 两个变体可用，与 Windows 侧不一致

**最小修复建议**：确认 Linux 上仅使用 `Path`/`Explicit` 变体；如未来需报告 bundled/program_files，需在 Linux 上补占位变体或重构枚举。

---

#### 问题 3（中）：`excluded_windows_shell` 依赖 `SystemRoot` 环境变量，缺失时硬编码 `C:\Windows`

**文件**：`src-tauri/crates/polaris-kernel/src/doctor/pi_shell.rs:189-191`

**证据**：
```rust
let system_root = ["SystemRoot", "windir"].into_iter()
    .filter_map(std::env::var_os).find(|value| !value.is_empty())
    .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
```

**问题**：`SystemRoot` 被改写或为空时回退 `C:\Windows`，但实际上 Windows 机器的 System32 路径不一定是 `C:\Windows\System32`（例如 PE 环境、不同盘符）。测试用例 `windows_exclusions_normalize_case_separators_and_directory_boundaries` 仅覆盖自定义路径，未覆盖 System32 真实位置不在 C 盘的情况。

**最小修复建议**：文档注明此回退为近似值；或在测试中显式验证 System32 实际路径与 `SystemRoot` 的关系。

---

#### 问题 4（中）：Linux 下 `path_contains_dir` 测试未验证空路径输入

**文件**：`src-tauri/crates/polaris-kernel/src/doctor/path_tests.rs:10-22`

**证据**：
```rust
assert!(!path_contains_dir("", "/usr/bin"));  // 空 PATH 测试存在
```

**问题**：测试断言存在，但 brief 中"补 Linux `path_contains_dir` 测试"未明确要求此项；空输入边界行为是否与 Windows 测试一致未经交叉验证。

**最小修复建议**：对照 Windows 测试确认空路径处理行为相同。

---

### 建议验证步骤（推断风险，非已有证据）

| 风险 | 验证方式 |
|------|----------|
| 问题 1（严重）：Linux 编译失败 | 在 Linux 机器执行 `cargo check -p polaris-kernel`，观察是否报 "variant `Bundled` is unknown" |
| 问题 2（中）：枚举变体不一致 | 检查 `ShellSource::as_str()` 在 Linux 能否返回 "bundled"（若调用会 panic 或编译失败） |
| 问题 3（中）：非 C 盘 Windows | 在 D 盘安装 Windows、系统盘非 C 的环境测试 doctor 输出 |

---

### 无问题确认

- **排除逻辑本身**（`excluded_windows_shell`）：`windows_components` 规范化大小写、斜杠、`.`/`..`、`\\?\` 前缀，逻辑覆盖 brief 中除"符号链接/目录联接指向 System32"外的所有场景
- **doctor 报告格式**：实际来源与 shell 路径均通过 `ResolvedShell` 传递，未发现不一致
- **空环境变量约定**：在配置文档写明（round1 triage 承诺），本轮 diff 未涉及但可接受
- **改动文件行数**：diff 涉及 4 文件，预计总行数 <500，无压行
