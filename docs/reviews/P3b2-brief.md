# 审查 brief：P3b 沙箱挂载最小化、容器安全基线、专家请求级传递

附 `P3b2-diff.patch`（非测试源码）。

## 背景
旧 `sandbox_start` 把整个 `~/Polaris` 读写挂到 `/workspace`，其中含 `data/providers.json`（明文模型 Key）、`data/host.key`/`device.key`（互联设备身份）、`data/collab.db`（账号库）。

## 验收条件
1. 只挂目标项目到 `/workspace`（可写）与知识库到 `/kb`（只读）；`data/`、密钥、账号库、`claude-home`、会话目录等一律不挂。挂载清单来自配置；代码兜底名单（至少 `~/Polaris/data`、`~/.polaris`、`~/.pi`、`~/.claude`、`~/.codex`、`~/.ssh`、`~/.aws`、`~/.gnupg`、`~/.kube`）配置只能追加不能移除。
2. 路由前校验容器：挂载 Destination 与读写属性（`/workspace` 是目标项目且可写，`/kb` 只读，无其他宿主挂载）；资源上限、`cap-drop=ALL`、`no-new-privileges`；不满足报 `SANDBOX_UNSAFE_CONTAINER`。
3. 旧宽挂载容器被识别并提示迁移、提供一键重建；不自动删除用户容器；旧 UI 不再报 `SANDBOX_PROJECT_REQUIRED`（默认当前会话工作目录）。
4. Pi 后端下显式专家/团队选择随本轮请求传递，不写项目 `CLAUDE.md`/`AGENTS.md`；旧后端行为不变。
5. 服务端形态（polaris-server）暂不支持沙箱：明确报错，不回落宿主执行。

## Claude 实测（沙箱外）
Rust 两壳 `cargo check` 通过（7 警告同基线）；`cargo test -p polaris-sandbox` 13/13；`cargo test -p polaris-kernel --lib --tests` 176 通过、4 失败为基线；另有 1 个偶发失败（`model_profile_image_and_idle_reconfiguration_are_on_the_wire`，约 1/7，单独运行 5/5 通过，疑为测试读取假 agentd 请求记录的时序竞争，已列入修复）；agentd 100/100；前端 vue-tsc 通过、vitest 23/23。既有超长文件净增：ChatComposer.vue +3、SandboxStatus.vue 净减、feishu.rs +1。

## 请重点审查
挂载与保护名单能否被绕过（符号链接指向受保护目录、Windows 盘符大小写、`~` 展开、项目目录本身位于受保护路径之下或之上、配置注入）；`docker run` 参数构造是否有注入风险（项目路径含空格/逗号/冒号/`=`）；重建流程是否可能误删或误用用户容器；专家请求级传递是否仍有写文件路径。
