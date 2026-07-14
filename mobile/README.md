# 北极星 · 安卓远程壳（Polaris Mobile）

一个**纯空壳**的安卓客户端：手机上**不运行任何后端**，登录账号后通过 HTTP/WS
远程连接你的**中控平台**（`polaris-server`），在手机上对话下发工作、管文件、
管协作项目。交互仿 Kimi —— 以对话为中心，其余能力**触发式**激活（底部「＋」面板）。

## 它怎么连上平台

平台本身就是一台 HTTP/WS 服务（`polaris-server`，Docker/服务器部署）。本 App 只做客户端：

| 用途 | 通道 |
|---|---|
| 下发命令（聊天/文件/工作，≈75 命令） | `POST {base}/api/invoke` + Bearer token |
| 流式回复 / 事件 | `GET {base}/ws?token=`（WebSocket） |
| 上传文件 | `POST {base}/api/upload` |
| 读文件/预览 | `GET {base}/api/file?path=&token=` |
| 账号/项目/任务 | `{base}/api/collab/*` |

**鉴权统一**：一次 collab 登录拿到的 JWT，既认 `/api/invoke` 又认协作接口
（见平台 `server.rs` 身份解析）。`base + token` 持久化在本地，重启自动恢复会话。

**第一屏是主机列表（UU远程式）**：每台保存过的主机一张卡（名称/地址/在线状态/
账号），进屏并行探活。点卡片 → 有存续登录态直接进对话；否则登录/注册/邀请码。
添加主机填「地址」（局域网 `IP:端口`，外网域名）或直接粘贴**分享码** `PLRS1-…`
（自动探活其中可达地址并预填邀请码）。多台主机各记各的账号 token，随点随切。

## 能力管控（可隐藏）

`src/lib/capabilities.ts` 是白名单式能力闸：出厂默认 ∩ 角色（owner 才见管理）∩
用户本地开关（设置页）。关掉的能力从底部导航与「更多」面板隐藏。核心对话不可关。

## 本地开发（浏览器里跑 UI）

```bash
cd mobile
npm install
npm run dev        # http://localhost:1431 ，填一台可达的 polaris-server 地址即可联调
```

## 出安卓 APK

本目录已备好 Capacitor 壳（`capacitor.config.ts`）。需要一台装了 **Android Studio /
Android SDK + JDK 17** 的机器：

```bash
cd mobile
npm install
npm run build                 # 产出 dist/（静态前端）
npx cap add android           # 首次:生成 android/ 原生工程
npx cap sync android          # 把 dist/ 同步进壳
npx cap open android          # 打开 Android Studio → Build APK / 直接跑真机
```

CLI 直出 debug 包（装好 SDK 后）：

```bash
cd android && ./gradlew assembleDebug
# 产物: android/app/build/outputs/apk/debug/app-debug.apk
```

> 明文 HTTP 已在 `capacitor.config.ts` 开启（`cleartext: true`），因局域网主机
> 多为 `http://IP:端口`。若只连 HTTPS 域名，可关掉以更安全。

## 目录

```
src/
  lib/
    net.ts          远程客户端:invoke / ws listen / upload / fileUrl / collab REST
    hosts.ts        多主机管理:设备列表 / 各主机会话 / 探活 / 激活切换
    auth.ts         会话:登录 / 注册 / 邀请码 / 登出(绑定到当前主机条目)
    chat.ts         聊天:send + chat:stream 流式装配 + 历史对话本地持久化
    convs.ts        历史对话存储(按主机分开,50会话×300消息上限,QuotaExceeded自动淘汰)
    preview.ts      全局预览状态(openPreview)
    capabilities.ts 能力管控白名单闸
    nav.ts toast.ts md.ts
  screens/          Hosts(主机列表,第一屏) / Chat / Files / Projects / Settings / Placeholder
  components/       ChatDrawer(左上☰抽屉:历史对话+切主机) / PreviewOverlay(全屏预览+横屏)
                    / BottomNav / TriggerSheet(＋面板) / Toast
```

## 现状（v1.1）

- ✅ 主机列表第一屏（UU远程式）：多主机保存、并行探活、各记各的账号、一点即连。
- ✅ 对话（流式+工具+产物+附件），左上角 ☰ 抽屉：历史对话（本地保存、按主机分开、
  点开续聊）、切换主机、新对话。
- ✅ App 内全屏预览：HTML 成品可交互体验、图片/视频/音频/文本/markdown 流式看，
  右上角 ⟳ 横竖屏切换（Manifest 未锁向，也可直接转手机）。原文件永远在主机，
  手机零文件存储 —— 只存主机列表与历史对话文本。
- ✅ 文件（搜索/浏览/上传→内嵌预览）、协作项目与任务（认领/送验）、能力管控。
- 🚧 迭代中：技能/知识库/语音/管理的**独立**移动界面（当前经对话触发或用桌面端）。
