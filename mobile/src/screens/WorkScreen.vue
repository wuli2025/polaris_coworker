<script setup lang="ts">
/**
 * 电脑项目 —— 手机上「进哪个项目干活」的选择页。
 *
 * 选中一个项目后,新对话会建在该项目名下;主机随即以它绑定的文件夹作为 claude 的
 * 工作目录(等同在电脑上 `cd <文件夹> && claude`)。所以这一页真正决定的是:
 * 手机上说的话,在电脑的哪个文件夹里生效。
 *
 * 也可以在这里补绑文件夹 —— 项目在电脑上建好却没绑目录时,不用跑回电脑前。
 */
import { onMounted, ref } from "vue";
import {
  projects,
  loading,
  activeProject,
  loadProjects,
  setWorkDir,
  createProject,
  dirName,
  type WorkProject,
} from "../lib/projects";
import { switchProject } from "../lib/chat";
import { toast, toastErr } from "../lib/toast";
import { go } from "../lib/nav";
import Ico from "../components/Ico.vue";

const err = ref("");
/** 正在编辑工作目录的项目 id(内联展开一行输入框)。 */
const editing = ref<string | null>(null);
const dirDraft = ref("");
const saving = ref(false);
/** 新建项目卡片。 */
const creating = ref(false);
const newName = ref("");
const newDir = ref("");

async function refresh() {
  err.value = "";
  try {
    await loadProjects();
  } catch (e) {
    err.value = (e as Error).message || "读取失败";
  }
}

async function enter(p: WorkProject) {
  await switchProject(p);
  toast(p.work_dir ? `已进入「${p.name}」· ${dirName(p.work_dir)}` : `已进入「${p.name}」`, "ok");
}

function startEdit(p: WorkProject, e: Event) {
  e.stopPropagation();
  editing.value = editing.value === p.id ? null : p.id;
  dirDraft.value = p.work_dir ?? "";
}

async function saveDir(p: WorkProject) {
  if (saving.value) return;
  saving.value = true;
  try {
    await setWorkDir(p.id, dirDraft.value);
    editing.value = null;
    toast(dirDraft.value.trim() ? "工作目录已绑定" : "已解除绑定", "ok");
  } catch (e) {
    toastErr(e); // 目录不存在时主机会把原路径回给你,照着改就行
  } finally {
    saving.value = false;
  }
}

async function doCreate() {
  const name = newName.value.trim();
  if (!name) {
    toast("先给项目起个名字");
    return;
  }
  if (saving.value) return;
  saving.value = true;
  try {
    const p = await createProject(name, newDir.value);
    creating.value = false;
    newName.value = "";
    newDir.value = "";
    await enter(p);
  } catch (e) {
    toastErr(e);
  } finally {
    saving.value = false;
  }
}

onMounted(refresh);
</script>

<template>
  <section class="wk">
    <header class="whead">
      <button class="wbtn" title="返回" @click="go('chat')"><Ico name="chevron" :size="18" /></button>
      <h1>电脑项目</h1>
      <button class="wbtn" title="刷新" @click="refresh"><Ico name="rotate" :size="17" /></button>
    </header>

    <p class="wtip">
      这些是<b>电脑上的项目</b>。选一个进去，之后对话里下发的活就在它绑定的文件夹里做 ——
      跟你在电脑上 <code>cd</code> 进那个目录再开 AI 一样。也可以直接在对话里说
      <b>「打开 ×× 项目」</b>。
    </p>

    <div v-if="loading && !projects.length" class="wcenter muted">读取中…</div>
    <div v-else-if="err" class="wcenter">
      <p class="werr">{{ err }}</p>
      <button class="wbig" @click="refresh">重试</button>
    </div>

    <div v-else class="wlist">
      <p v-if="!projects.length && !creating" class="wempty faint">
        电脑上还没有项目。新建一个，并绑上你的代码/资料文件夹。
      </p>

      <div
        v-for="p in projects"
        :key="p.id"
        class="pcard"
        :class="{ on: p.id === activeProject?.id }"
        @click="enter(p)"
      >
        <span class="pico"><Ico name="folder" :size="19" /></span>
        <span class="pmeta">
          <span class="pname">
            {{ p.name }}
            <span v-if="p.id === activeProject?.id" class="pnow">当前</span>
          </span>
          <span class="pdir faint">{{ p.work_dir || "未绑定文件夹 · 用主机默认目录" }}</span>
        </span>
        <button class="pedit" title="工作目录" @click="startEdit(p, $event)">
          <Ico name="settings" :size="16" />
        </button>

        <div v-if="editing === p.id" class="pform" @click.stop>
          <label class="flabel faint">工作目录（电脑上的绝对路径，留空=解绑）</label>
          <input
            v-model="dirDraft"
            class="finput"
            placeholder="D:\\code\\my-project 或 /Users/me/code/my-project"
            autocapitalize="off"
            autocomplete="off"
            spellcheck="false"
          />
          <div class="frow">
            <button class="fbtn ghost" @click="editing = null">取消</button>
            <button class="fbtn" :disabled="saving" @click="saveDir(p)">
              {{ saving ? "保存中…" : "保存" }}
            </button>
          </div>
        </div>
      </div>

      <!-- 新建 -->
      <div v-if="creating" class="pcard new" @click.stop>
        <div class="pform open">
          <label class="flabel faint">项目名字</label>
          <input v-model="newName" class="finput" placeholder="比如：公司官网" />
          <label class="flabel faint">工作目录（可留空，之后再绑）</label>
          <input
            v-model="newDir"
            class="finput"
            placeholder="D:\\code\\my-project"
            autocapitalize="off"
            autocomplete="off"
            spellcheck="false"
          />
          <div class="frow">
            <button class="fbtn ghost" @click="creating = false">取消</button>
            <button class="fbtn" :disabled="saving" @click="doCreate">
              {{ saving ? "创建中…" : "创建并进入" }}
            </button>
          </div>
        </div>
      </div>
      <button v-else class="padd" @click="creating = true">
        <Ico name="plus" :size="16" /> 新建项目
      </button>
    </div>
  </section>
</template>

<style scoped>
.wk {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.whead {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: calc(10px + var(--safe-top)) 12px 8px;
}
.whead h1 {
  flex: 1;
  margin: 0;
  font-size: 17px;
  font-weight: 650;
}
.wbtn {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: var(--bg-elev2);
  color: var(--text-dim);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.wtip {
  margin: 0 14px 10px;
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--text-dim);
}
.wtip code {
  font-size: 11.5px;
  padding: 1px 5px;
  border-radius: 6px;
  background: var(--bg-elev2);
}
.wcenter {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px;
  text-align: center;
}
.werr {
  color: #ff8f8f;
  margin: 0;
}
.wbig {
  padding: 10px 22px;
  border-radius: 12px;
  background: var(--accent);
  color: #fff;
}
.wlist {
  flex: 1;
  overflow-y: auto;
  padding: 0 14px calc(20px + var(--safe-bottom));
}
.wempty {
  text-align: center;
  padding: 36px 10px;
  line-height: 1.7;
}
.pcard {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 13px 14px;
  margin-bottom: 10px;
  border-radius: 14px;
  background: var(--bg-elev);
  border: 1px solid var(--line);
  cursor: pointer;
}
.pcard.on {
  border-color: var(--accent);
  background: var(--accent-soft, var(--bg-elev));
}
.pcard:active {
  filter: brightness(0.94);
}
.pico {
  width: 34px;
  height: 34px;
  border-radius: 11px;
  display: grid;
  place-items: center;
  background: var(--bg-elev2);
  color: var(--text-dim);
  flex-shrink: 0;
}
.pcard.on .pico {
  background: linear-gradient(140deg, var(--accent), var(--accent-2));
  color: #fff;
}
.pmeta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.pname {
  font-weight: 600;
  font-size: 14.5px;
  display: flex;
  align-items: center;
  gap: 7px;
}
.pnow {
  font-size: 10.5px;
  font-weight: 500;
  padding: 2px 7px;
  border-radius: 999px;
  background: var(--accent);
  color: #fff;
}
.pdir {
  font-size: 11.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  direction: rtl; /* 路径太长时截头留尾:末段目录名才是用户认得的那个 */
  text-align: left;
}
.pedit {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  color: var(--text-faint);
  display: grid;
  place-items: center;
  flex-shrink: 0;
}
.pform {
  width: 100%;
  margin-top: 4px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
  cursor: default;
}
.pform.open {
  border-top: 0;
  padding-top: 0;
  margin-top: 0;
}
.flabel {
  display: block;
  font-size: 11.5px;
  margin: 6px 2px 5px;
}
.finput {
  width: 100%;
  padding: 10px 12px;
  border-radius: 11px;
  background: var(--bg-elev2);
  border: 1px solid var(--line);
  color: var(--text);
  font-size: 13.5px;
}
.frow {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-top: 10px;
}
.fbtn {
  padding: 8px 18px;
  border-radius: 10px;
  background: var(--accent);
  color: #fff;
  font-size: 13px;
}
.fbtn.ghost {
  background: var(--bg-elev2);
  color: var(--text-dim);
}
.fbtn:disabled {
  opacity: 0.6;
}
.padd {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  width: 100%;
  padding: 12px;
  border-radius: 14px;
  border: 1px dashed var(--line);
  color: var(--text-dim);
  font-size: 13.5px;
}
.pcard.new {
  display: block;
  cursor: default;
}
</style>
