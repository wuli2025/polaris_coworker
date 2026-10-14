use super::*;

    /// 占位符必须全部被填掉 —— 漏一个就会把 `DEPS_BASE/xxx.msi` 这种字面量当 URL 去下载。
    #[test]
    fn scripts_have_no_unfilled_placeholders() {
        let mut all = vec![
            ("node", node_install_script()),
            ("pwsh", pwsh_install_script()),
        ];
        #[cfg(windows)]
        {
            all.push(("uv", uv_install_script()));
            all.push(("claude", claude_install_script()));
        }
        #[cfg(target_os = "macos")]
        all.push(("mac-uv", mac_uv_install_script()));
        #[cfg(not(windows))]
        all.push(("unix-claude", fill(UNIX_CLAUDE_INSTALL_SCRIPT)));
        all.push(("node-portable", node_portable_script()));
        for (name, s) in &all {
            for ph in [
                "DEPS_BASE", "MSI_MAGIC", "ZIP_MAGIC", "GZ_MAGIC", "CLAUDE_VER", "PWSH_VER",
                "NODE_VER", "UV_VER",
            ] {
                assert!(!s.contains(ph), "{name} 脚本里还留着未替换的占位符 {ph}");
            }
            assert!(
                s.contains(DEPS_BASE),
                "{name} 脚本应把自家 R2 源排进候选 (否则镜像白传)"
            );
        }
    }

    /// Claude 直装脚本必须走「平台包 tgz + tar 解压」而**不是** `npm i -g` —— 后者是被替掉的卡死路径。
    #[test]
    fn claude_install_is_direct_tgz_not_npm() {
        #[cfg(windows)]
        let s = claude_install_script();
        #[cfg(not(windows))]
        let s = fill(UNIX_CLAUDE_INSTALL_SCRIPT);
        {
            assert!(s.contains("claude-code-"), "应按平台包名抓 tgz");
            assert!(s.contains(".tgz"), "应下载 tgz");
            assert!(!s.contains("npm install"), "默认路径不该再调 npm i -g");
            // 每个源必须端到端自证: 解压后真跑一次 --version, 而非只校验体积/魔数
            assert!(s.contains("--version"), "每个源应跑 --version 自证二进制能用");
            // 四源、失败域各不相同, 且 R2 打头 (不开梯子最快): R2 → npmmirror → yarn → npmjs
            let r2 = s.find(DEPS_BASE).expect("应含 R2 源");
            let mirror = s.find("registry.npmmirror.com").expect("应含 npmmirror 源");
            let yarn = s.find("registry.yarnpkg.com").expect("应含 yarn 镜像源");
            let npmjs = s.find("registry.npmjs.org").expect("应含 npmjs 兜底源");
            assert!(
                r2 < mirror && mirror < yarn && yarn < npmjs,
                "源顺序应为 R2 → npmmirror → yarn → npmjs"
            );
        }
    }

    /// R2 源必须排在公共代理**前面** —— 顺序错了等于没镜像 (用户仍先撞 gh-proxy 那类不稳定源)。
    #[test]
    fn r2_source_comes_first() {
        for (name, s) in [("node", node_install_script()), ("pwsh", pwsh_install_script())] {
            let r2 = s.find(DEPS_BASE).expect("应含 R2 源");
            for other in ["ghfast.top", "ghproxy.net", "cdn.npmmirror.com", "nodejs.org"] {
                if let Some(i) = s.find(other) {
                    assert!(r2 < i, "{name}: R2 源应排在 {other} 之前");
                }
            }
        }
        // 实测已 500 的 gh-proxy.com 不该再出现在下载候选里。只认 `$urls` 里的 URL 字面量形态,
        // 别把正文里解释「为什么摘掉它」的那句注释也算进来。
        assert!(!pwsh_install_script().contains("\"https://gh-proxy.com/"));
    }

    /// 关掉 IWR 进度条是「PowerShell 7 有些概率装不上」的主修 —— 实测 584KB/s → 4266KB/s。
    /// 三个 Windows 脚本一个都不能漏, 故在此锁死。
    #[test]
    fn windows_scripts_disable_progress_bar() {
        let mut all = vec![
            node_install_script(),
            pwsh_install_script(),
            node_portable_script(),
        ];
        #[cfg(windows)]
        {
            all.push(uv_install_script());
            all.push(claude_install_script());
        }
        for s in &all {
            assert!(s.contains("$ProgressPreference = 'SilentlyContinue'"));
        }
    }

    /// 把最终生成的安装脚本原文 dump 到临时目录, 供真机手动跑「下载段」核对
    /// (R2 源是否命中、魔数校验是否放行)。真下载几十上百 MB, 故不进常规测试。
    /// `cargo test -p polaris-kernel dump_install_scripts -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_install_scripts() {
        let dir = std::env::temp_dir();
        let mut all = vec![
            ("node", node_install_script()),
            ("pwsh", pwsh_install_script()),
        ];
        #[cfg(windows)]
        all.push(("uv", uv_install_script()));
        #[cfg(target_os = "macos")]
        all.push(("mac-uv", mac_uv_install_script()));
        all.push(("node-portable", node_portable_script()));
        for (name, src) in &all {
            let ext = if name.starts_with("mac") { "sh" } else { "ps1" };
            let p = dir.join(format!("polaris_{name}_install.{ext}"));
            let mut bytes = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM: 见上面解析测试里的说明
            bytes.extend_from_slice(src.as_bytes());
            std::fs::write(&p, &bytes).unwrap();
            println!("{name} -> {}", p.display());
        }
    }

    /// 内嵌 PowerShell 脚本的语法错误以前只有「用户真的点安装」时才暴露。
    /// 这里用 PowerShell 自带的 AST 解析器在**测试期**就把它抓出来。
    #[test]
    #[cfg(windows)]
    fn install_scripts_parse_as_valid_powershell() {
        for (name, src) in [
            ("node", node_install_script()),
            ("pwsh", pwsh_install_script()),
            ("uv", uv_install_script()),
            ("claude", claude_install_script()),
            ("node-portable", node_portable_script()),
        ] {
            let f = std::env::temp_dir().join(format!("polaris_test_{name}_install.ps1"));
            // 必须写 UTF-8 BOM: Windows PowerShell 5.1 读**无 BOM** 的 .ps1 会按 ANSI(GBK) 解,
            // 脚本里的中文当场被搅碎、引号配不上对, 报一堆假的语法错。
            // (运行时那条路不经文件 —— build_powershell 用 `-Command` 传, Rust 经 CreateProcessW
            //  给的是 UTF-16, 没这个问题。这里是为了让 ParseFile 看到跟内存里一样的字节。)
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(src.as_bytes());
            std::fs::write(&f, &bytes).expect("写临时脚本");
            // 单引号字符串里反斜杠是字面量, 无需转义; 只需把路径里的单引号翻倍。
            let path = f.display().to_string().replace('\'', "''");
            let script = format!(
                "$errs = $null; \
                 $null = [System.Management.Automation.Language.Parser]::ParseFile('{path}', [ref]$null, [ref]$errs); \
                 if ($errs -and $errs.Count -gt 0) {{ $errs | ForEach-Object {{ Write-Output $_.ToString() }}; exit 1 }}"
            );
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .output()
                .expect("跑 PowerShell 解析器");
            assert!(
                out.status.success(),
                "{name} 安装脚本语法有错:\n{}",
                String::from_utf8_lossy(&out.stdout)
            );
            let _ = std::fs::remove_file(&f);
        }
    }
