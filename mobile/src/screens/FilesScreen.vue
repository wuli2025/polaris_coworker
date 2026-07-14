<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke, upload } from "../lib/net";
import { toast, toastErr } from "../lib/toast";
import { openPreview } from "../lib/preview";

interface FileCard {
  id: number;
  abspath: string;
  name: string;
  title: string;
  kind: string;
  sizeH: string;
  mtime: number;
}
interface GridPage {
  items: FileCard[];
  total: number;
}

const items = ref<FileCard[]>([]);
const total = ref(0);
const query = ref("");
const loading = ref(false);
const page = ref(0);
const fileInput = ref<HTMLInputElement | null>(null);

const ICON: Record<string, string> = {
  image: "🖼️",
  video: "🎬",
  audio: "🎵",
  doc: "📄",
  text: "📝",
  archive: "🗜️",
  other: "📦",
};

async function load(reset = true) {
  if (loading.value) return;
  loading.value = true;
  try {
    if (reset) page.value = 0;
    const r = await invoke<GridPage>("file_grid", {
      root: null,
      clusterId: null,
      kind: null,
      lang: null,
      sort: "recent",
      query: query.value.trim() || null,
      page: page.value,
      pageSize: 40,
    });
    items.value = reset ? r.items : [...items.value, ...r.items];
    total.value = r.total;
  } catch (e) {
    toastErr(e);
  } finally {
    loading.value = false;
  }
}
function more() {
  page.value++;
  load(false);
}

async function doUpload() {
  fileInput.value?.click();
}
async function onFiles(e: Event) {
  const files = (e.target as HTMLInputElement).files;
  if (!files?.length) return;
  try {
    const up = await upload(files);
    toast(`已上传 ${up.length} 个文件到主机`, "ok");
    load(true);
  } catch (e) {
    toastErr(e);
  } finally {
    (e.target as HTMLInputElement).value = "";
  }
}

onMounted(() => load(true));
</script>

<template>
  <div class="files">
    <header class="bar">
      <div class="title">文件</div>
      <button class="icon" title="上传" @click="doUpload">⬆</button>
    </header>

    <div class="search">
      <input
        v-model="query"
        class="field"
        placeholder="搜索远端文件…"
        @keyup.enter="load(true)"
      />
    </div>

    <div class="list">
      <p v-if="!items.length && !loading" class="empty muted">暂无文件</p>
      <button v-for="f in items" :key="f.id" class="item" @click="openPreview(f.abspath)">
        <span class="fi">{{ ICON[f.kind] || "📦" }}</span>
        <span class="meta">
          <span class="nm">{{ f.title || f.name }}</span>
          <span class="sub faint">{{ f.sizeH }} · {{ f.kind }}</span>
        </span>
      </button>
      <button
        v-if="items.length < total"
        class="btn ghost more"
        :disabled="loading"
        @click="more"
      >
        {{ loading ? "加载中…" : `加载更多（${items.length}/${total}）` }}
      </button>
    </div>

    <input ref="fileInput" type="file" multiple hidden @change="onFiles" />
  </div>
</template>

<style scoped>
.files {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: calc(10px + var(--safe-top)) 16px 10px;
  border-bottom: 1px solid var(--line);
}
.title {
  font-weight: 600;
}
.icon {
  font-size: 20px;
  color: var(--text-dim);
  width: 34px;
  height: 34px;
}
.search {
  padding: 12px 14px;
}
.list {
  flex: 1;
  overflow-y: auto;
  padding: 0 14px 20px;
}
.empty {
  text-align: center;
  padding: 40px;
}
.item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 12px;
  border-bottom: 1px solid var(--line);
  color: var(--text);
  text-align: left;
}
.fi {
  font-size: 26px;
}
.meta {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.nm {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sub {
  font-size: 12px;
}
.more {
  width: 100%;
  margin-top: 14px;
}
</style>
