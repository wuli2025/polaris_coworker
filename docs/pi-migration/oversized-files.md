# 超过 500 行的自有代码基线

基线：2026-10-14，HEAD `0be46d7474d4ec29f764cf253036eb79fda482b8`。

范围：从 `git ls-files -z` 枚举版本管理内的文本代码，按实际逻辑行数统计（CRLF 视为一行，末行无换行仍计一行）。
包含 Rust、TS/TSX、Vue、JS/MJS、Python、CSS、shell/PowerShell、HTML 等源码，以及自有脚本/测试/示例；不把 Markdown、JSON 数据、锁文件和素材当业务代码。
排除 node_modules、target、dist、vendor、generated 目录和生成产物；构建生成的 gift_key.rs 在 OUT_DIR（`src-tauri/crates/polaris-kernel/build.rs:9`），不算源文件。
卡片表虽是大量静态数据，仍是受版本管理的 Rust 源码，纳入统计（`src-tauri/src/expert/expert_groups.rs:47`）。

共 **124 个文件**。最大为 `src-tauri/crates/polaris-collab/src/collab/http.rs`，4822 行；证据见每行的源码起止行号。

## 标记口径

- A：按已证实的依赖预计 P1–P6 会触及，修改时按规划拆到 ≤500 行；不是本次 P0 已修改。
- B：兼容/回归检查对象；若接口保持兼容可不修改，暂不要求拆分。
- C：当前已识别的 Pi/互联改造范围外，保留原状，不为行数主动拆分。
- A 的原因附具体源码行；B/C 的边界是范围决策，并非“绝对不会受影响”的结论。总体范围依据 `docs/plans/2026-10-14-pi-refactor.md:60` 至 `:64`，不触及不拆规则见 `:71`。

| 文件:起止行 | 行数 | 标记 | 阶段 / 触及原因与证据 |
| --- | ---: | --- | --- |
| `src-tauri/crates/polaris-collab/src/collab/http.rs:1–4822` | 4822 | A | P6：账号/设备路由与 relay 下发；`src-tauri/crates/polaris-collab/src/collab/http.rs:3460` |
| `src-tauri/src/apihub.rs:1–3487` | 3487 | A | P2/P4/P5：命令分发、认证与环境接口；`src-tauri/src/apihub.rs:2218` |
| `src/features/interconnect/InterconnectView.vue:1–2756` | 2756 | A | P6：设备互联与执行入口；`src/features/interconnect/InterconnectView.vue:1569` |
| `src/tauri.ts:1–2631` | 2631 | A | P2–P6：命令类型与兼容入口；`src/tauri.ts:1501` |
| `src/components/chat/ChatComposer.vue:1–2556` | 2556 | A | P2/P5：模式、权限与运行时入口；`src/components/chat/ChatComposer.vue:513` |
| `src-tauri/crates/polaris-forge/src/forge/pptx_native.rs:1–2423` | 2423 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/provider/store.rs:1–2292` | 2292 | A | P4：供应商到运行时映射；`src-tauri/crates/polaris-kernel/src/provider/store.rs:1637` |
| `src-tauri/crates/polaris-kernel/src/integrations/codex_proxy.rs:1–2269` | 2269 | A | P4：旧路由后端隔离；`src-tauri/crates/polaris-kernel/src/integrations/codex_proxy.rs:71` |
| `src-tauri/src/fsmount.rs:1–2257` | 2257 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-kernel/src/chat/pipeline.rs:1–2080` | 2080 | A | P2/P3：后端执行与事件适配；`src-tauri/crates/polaris-kernel/src/chat/pipeline.rs:150` |
| `src/components/WikiBrowse.vue:1–2003` | 2003 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/src/expert/expert_groups.rs:1–1948` | 1948 | A | P3：上下文模板引用字段；`src-tauri/src/expert/expert_groups.rs:18` |
| `src/components/ArtifactEditor.vue:1–1878` | 1878 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/src/templates/skills/wechat-md-typesetter/scripts/wechat_yiban.py:1–1871` | 1871 | A | P5：嵌套 Claude CLI 调用；`src-tauri/src/templates/skills/wechat-md-typesetter/scripts/wechat_yiban.py:1670` |
| `src/components/RightDrawer.vue:1–1767` | 1767 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/src/mesh.rs:1–1631` | 1631 | A | P6：登录入网与设备对账；`src-tauri/src/mesh.rs:810` |
| `src/components/VideoCourseStudio.vue:1–1625` | 1625 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/SenseApi.vue:1–1557` | 1557 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/MediaOps.vue:1–1540` | 1540 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-collab/src/collab/auth.rs:1–1534` | 1534 | A | P6：账号密码认证；`src-tauri/crates/polaris-collab/src/collab/auth.rs:709` |
| `src-tauri/crates/polaris-collab/src/collab/authority.rs:1–1490` | 1490 | A | P6：权威断言与准入；`src-tauri/crates/polaris-collab/src/collab/authority.rs:766` |
| `src-tauri/src/beam.rs:1–1447` | 1447 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src/components/ProviderDock.vue:1–1410` | 1410 | A | P4：订阅登录与模型选择；`src/components/ProviderDock.vue:270` |
| `src/components/Sidebar.vue:1–1349` | 1349 | A | P3：上下文入口；`src/components/Sidebar.vue:216` |
| `src-tauri/src/appproxy.rs:1–1334` | 1334 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src/components/OnboardingWizard.vue:1–1292` | 1292 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-kernel/src/chat/prompt.rs:1–1278` | 1278 | A | P2/P3：CLI 提示与工具约定；`src-tauri/crates/polaris-kernel/src/chat/prompt.rs:1145` |
| `src-tauri/crates/polaris-kernel/src/doctor/verify.rs:1–1270` | 1270 | A | P5：Agent 深度校验；`src-tauri/crates/polaris-kernel/src/doctor/verify.rs:1031` |
| `src/components/FileCenter.vue:1–1265` | 1265 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `mobile/src/screens/ChatScreen.vue:1–1243` | 1243 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-fable/src/fable/inventory/scan.rs:1–1208` | 1208 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/chat/session_pool.rs:1–1170` | 1170 | A | P2：旧进程池隔离；`src-tauri/crates/polaris-kernel/src/chat/session_pool.rs:551` |
| `src-tauri/crates/polaris-kernel/src/integrations/feishu.rs:1–1155` | 1155 | A | P5：chat 调用与文案；`src-tauri/crates/polaris-kernel/src/integrations/feishu.rs:836` |
| `src-tauri/crates/polaris-collab/src/collab/tunnel.rs:1–1144` | 1144 | A | P6：relay 与设备隧道；`src-tauri/crates/polaris-collab/src/collab/tunnel.rs:256` |
| `src/components/EnvDoctor.vue:1–1143` | 1143 | A | P5：CLI 安装与校验 UI；`src/components/EnvDoctor.vue:268` |
| `src-tauri/crates/polaris-forge/src/forge/pptx.rs:1–1142` | 1142 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/features/collab/TaskBoard.vue:1–1136` | 1136 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/doctor/install.rs:1–1118` | 1118 | A | P5：默认 Agent 安装器；`src-tauri/crates/polaris-kernel/src/doctor/install.rs:36` |
| `src-tauri/src/expert.rs:1–1100` | 1100 | A | P3：专家上下文装载/应用；`src-tauri/src/expert.rs:407` |
| `src/features/collab/CollabView.vue:1–1099` | 1099 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src/features/collab/api.ts:1–1095` | 1095 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src/components/chat/TurnItem.vue:1–1077` | 1077 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-fable/src/fable/echo.rs:1–1076` | 1076 | A | P5：headless 调用迁移；业务处理保留；`src-tauri/crates/polaris-fable/src/fable/echo.rs:660` |
| `src-tauri/crates/polaris-fable/src/fable/scan.rs:1–1058` | 1058 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/files/llm.rs:1–1037` | 1037 | A | P5：headless 调用迁移；业务处理保留；`src-tauri/crates/polaris-fable/src/fable/files/llm.rs:262` |
| `src-tauri/crates/polaris-fable/src/fable/mod.rs:1–1033` | 1033 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/src/voice/mod.rs:1–1019` | 1019 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/sense.rs:1–1005` | 1005 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `marketing-site/app/globals.css:1–1003` | 1003 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/conv.rs:1–990` | 990 | A | P2/P3：上下文文件及会话回收；`src-tauri/crates/polaris-kernel/src/conv.rs:194` |
| `src/components/NasManager.vue:1–959` | 959 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/SkillCenter.vue:1–956` | 956 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/VoiceSettings.vue:1–940` | 940 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/KnowledgeGraph.vue:1–938` | 938 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/features/collab/stores/collab.ts:1–924` | 924 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src/lib/slidesSpec.ts:1–918` | 918 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/index/build.rs:1–906` | 906 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/DeckStudio.vue:1–902` | 902 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/files/tests.rs:1–890` | 890 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-collab/src/collab/identity.rs:1–889` | 889 | A | P6：设备身份与访问码；`src-tauri/crates/polaris-collab/src/collab/identity.rs:32` |
| `src-tauri/crates/polaris-fable/src/fable/inventory/tests.rs:1–877` | 877 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/AddProviderModal.vue:1–844` | 844 | A | P4：旧 env 形态配置；`src/components/AddProviderModal.vue:219` |
| `src-tauri/crates/polaris-collab/src/collab/checks.rs:1–844` | 844 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/McpConfigModal.vue:1–841` | 841 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/files/cluster.rs:1–802` | 802 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/ClaudeMdPanel.vue:1–783` | 783 | A | P3：旧上下文编辑器；`src/components/ClaudeMdPanel.vue:171` |
| `src-tauri/src/project.rs:1–749` | 749 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-collab/src/collab/mergectl.rs:1–749` | 749 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-collab/src/collab/db.rs:1–745` | 745 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-kernel/src/chat/artifacts.rs:1–723` | 723 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-kernel/src/doctor/probe.rs:1–706` | 706 | A | P5：Agent 检测；`src-tauri/crates/polaris-kernel/src/doctor/probe.rs:427` |
| `src-tauri/crates/polaris-collab/src/collab/fsface.rs:1–703` | 703 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-fable/src/fable/ontology.rs:1–694` | 694 | A | P5：headless 调用迁移；业务处理保留；`src-tauri/crates/polaris-fable/src/fable/ontology.rs:476` |
| `src-tauri/crates/polaris-forge/src/forge/mod.rs:1–691` | 691 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/FeishuSettings.vue:1–687` | 687 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-fable/src/fable/files/overview.rs:1–687` | 687 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/kb/enrich.rs:1–685` | 685 | A | P5：headless 调用迁移；业务处理保留；`src-tauri/crates/polaris-fable/src/kb/enrich.rs:205` |
| `src-tauri/crates/polaris-collab/src/collab/grants.rs:1–684` | 684 | A | P6：同账号默认信任策略；`src-tauri/crates/polaris-collab/src/collab/grants.rs:167` |
| `src/components/filecenter/FolderPickerModal.vue:1–675` | 675 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/stores/chat.ts:1–665` | 665 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-collab/src/collab/gitea.rs:1–664` | 664 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/UsageBoard.vue:1–660` | 660 | A | P4：账本来源说明；`src/components/UsageBoard.vue:252` |
| `src/components/Automation.vue:1–658` | 658 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/src/lib.rs:1–651` | 651 | A | P2/P4/P5：初始化、命令表、退出回收；`src-tauri/src/lib.rs:201` |
| `mobile/src/screens/HostsScreen.vue:1–643` | 643 | A | P6：主机账号密码登录；`mobile/src/screens/HostsScreen.vue:213` |
| `src-tauri/crates/polaris-kernel/src/integrations/nas.rs:1–640` | 640 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/sched.rs:1–632` | 632 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/ChatPanel.vue:1–624` | 624 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/src/server.rs:1–615` | 615 | A | P2/P5：旧后端健康报告与启动；`src-tauri/src/server.rs:472` |
| `src-tauri/src/hosting.rs:1–606` | 606 | A | P6：主机入口与设备访问；`src-tauri/src/hosting.rs:338` |
| `src/stores/automation.ts:1–601` | 601 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-collab/src/collab/workset.rs:1–601` | 601 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/src/palette.rs:1–599` | 599 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/src/updater.rs:1–596` | 596 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/WebStudio.vue:1–595` | 595 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/components/WorkflowPackModal.vue:1–585` | 585 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/App.vue:1–585` | 585 | A | P5：默认环境托管提示；`src/App.vue:194` |
| `src/components/AutomationModal.vue:1–584` | 584 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/src/exec.rs:1–578` | 578 | A | P3/P6：执行允许列表与后端命令；`src-tauri/src/exec.rs:135` |
| `src/style.css:1–573` | 573 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/provider/usage.rs:1–562` | 562 | A | P4：用量来源；`src-tauri/crates/polaris-kernel/src/provider/usage.rs:96` |
| `src-tauri/src/templates/skills/wechat-tasks/scripts/wx_mac.py:1–560` | 560 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/files/profile.rs:1–558` | 558 | A | P5：headless 调用迁移；业务处理保留；`src-tauri/crates/polaris-fable/src/fable/files/profile.rs:404` |
| `src/components/Settings.vue:1–556` | 556 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `mobile/src/lib/net.ts:1–551` | 551 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `mobile/src/components/PreviewOverlay.vue:1–547` | 547 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-collab/src/collab/gateway.rs:1–544` | 544 | A | P6：账号注册主机与网关；`src-tauri/crates/polaris-collab/src/collab/gateway.rs:167` |
| `src-tauri/crates/polaris-kernel/src/convert.rs:1–534` | 534 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/index/ivf.rs:1–534` | 534 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-collab/examples/email_login_probe.rs:1–533` | 533 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/skills/intent.rs:1–532` | 532 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/kb/ingest.rs:1–530` | 530 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-fable/src/fable/index/math.rs:1–528` | 528 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `mobile/src/components/ChatDrawer.vue:1–525` | 525 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-collab/src/collab/tasks.rs:1–524` | 524 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/stores/app.ts:1–520` | 520 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src/features/sandbox/components/SandboxStatus.vue:1–517` | 517 | A | P3：沙箱执行状态说明；`src/features/sandbox/components/SandboxStatus.vue:241` |
| `src-tauri/crates/polaris-forge/src/forge/tts.rs:1–515` | 515 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/features/collab/CollabAdmin.vue:1–514` | 514 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src-tauri/crates/polaris-kernel/src/provider/image_store.rs:1–514` | 514 | B | 兼容回归；不预定改实现，入口/类型见本文件 :1 |
| `src-tauri/crates/polaris-kernel/src/provider/claude_login.rs:1–514` | 514 | A | P4：OAuth 接口；`src-tauri/crates/polaris-kernel/src/provider/claude_login.rs:84` |
| `src/features/collab/ProjectHome.vue:1–511` | 511 | C | 范围外；文件职责见本文件 :1；仅登记行数 |
| `src/features/auth/AccountCenterModal.vue:1–505` | 505 | A | P6：账号中心登录入口；`src/features/auth/AccountCenterModal.vue:1` |
| `src/components/UpdatePanel.vue:1–501` | 501 | C | 范围外；文件职责见本文件 :1；仅登记行数 |

## 使用限制

本清单不代替调用点表：headless.rs、codex_login.rs、wiki/compile.rs、sandbox/commands.rs 等虽不超 500 行，也是真迁移对象；见 [call-sites.md](call-sites.md)。
P0 只新增文档，上表所有业务源码保持不动。后续每次扩大范围时更新 A/B/C 与证据，不据关键词误命中把整个目录列为必拆。
