import type { AttachedFile, PermissionMode } from "../tauri";
import type { ExpertSelection } from "../lib/expertSelection";

export interface Bubble {
  role: "user" | "assistant" | "tool";
  text: string;
  tool?: string;
  /** 工具输入摘要(命令/路径/检索词一行,后端 tool 事件给出) → pill 可展开看 */
  toolDetail?: string;
  /** 本条 assistant 消息生成的成品文件（绝对路径，正斜杠） */
  artifacts?: string[];
  /** 本条 user 消息携带的上传附件 */
  files?: AttachedFile[];
  /** 消息时间(ms);历史消息来自后端 created_at,实时消息为收到时刻 */
  at?: number;
  /** 后端 error 事件合成的展示用错误气泡。delta 不得拼进来——否则 stderr 一行
   *  告警之后的全部正文都会被追加进错误气泡、整段被前端当错误吞掉。 */
  err?: boolean;
  /** 展示严重度：stderr 等过程诊断是 warning，只有本轮真实失败才是 error。 */
  severity?: "warning" | "error";
}

export interface ChatRuntimeOptions {
  permissionMode: PermissionMode;
  skillIds: string[];
  goal?: string;
  dynamicWorkflow?: boolean;
  useKb?: boolean;
  batchBuild?: boolean;
  batchSize?: number;
  agentMode?: string;
  expertSelection?: ExpertSelection;
  workMode?: string;
  providerId?: string;
}
