<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, fmtTime, thumbUrl, type AlbumItem, type AiStatus, type ScoredMedia } from "../api";

const emit = defineEmits<{ close: []; refresh: [] }>();

const status = ref<AiStatus | null>(null);
const busy = ref(false);
const progressMsg = ref("");
const error = ref("");

const tab = ref<"search" | "select" | "albums">("search");

// 搜索
const searchQuery = ref("");
const searchResults = ref<ScoredMedia[]>([]);
const searchDone = ref(false);

// 智能精选
const selectPrompt = ref("");
const selectPool = ref<"all" | "keep">("all");
const selectResults = ref<ScoredMedia[]>([]);
const selectParsed = ref("");
const selectAlbumName = ref("");
const albumSaved = ref(false);

// 精选集
const albums = ref<AlbumItem[]>([]);
const openAlbum = ref<{ name: string; items: ScoredMedia[] } | null>(null);

async function refreshStatus() {
  status.value = await api.clipStatus();
  if (tab.value === "albums") await refreshAlbums();
}
onMounted(refreshStatus);

async function refreshAlbums() {
  albums.value = await api.listAlbums();
}

async function downloadModels() {
  busy.value = true;
  error.value = "";
  progressMsg.value = "准备下载…";
  try {
    await api.clipDownload();
    pollClip("模型下载完成");
  } catch (e) {
    fail(String(e));
  }
}

async function buildIndex() {
  busy.value = true;
  error.value = "";
  try {
    await api.embedAll();
    pollClip("语义索引完成");
  } catch (e) {
    fail(String(e));
  }
}

function pollClip(doneMsg: string) {
  const timer = window.setInterval(async () => {
    const s = await api.getScanStatus();
    if (!s.running) {
      window.clearInterval(timer);
      busy.value = false;
      progressMsg.value = "";
      await refreshStatus();
      // 事件里带错误时 phase 非"完成"
      if (s.phase && s.phase !== "完成") error.value = s.phase;
      else progressMsg.value = doneMsg;
      window.setTimeout(() => (progressMsg.value = ""), 4000);
    } else {
      progressMsg.value = s.current || s.phase;
    }
  }, 500);
}

function fail(e: string) {
  error.value = e;
  busy.value = false;
  progressMsg.value = "";
}

async function doSearch() {
  error.value = "";
  searchDone.value = false;
  try {
    searchResults.value = await api.semanticSearch(searchQuery.value, 60);
    searchDone.value = true;
  } catch (e) {
    error.value = String(e);
  }
}

async function doSelect() {
  error.value = "";
  selectResults.value = [];
  albumSaved.value = false;
  try {
    const r = await api.smartSelect(selectPrompt.value, selectPool.value);
    selectResults.value = r.items;
    selectParsed.value = `理解：挑 ${r.parsedCount} 张「${r.parsedText}」`;
    selectAlbumName.value = selectPrompt.value.slice(0, 20);
  } catch (e) {
    error.value = String(e);
  }
}

async function saveAlbum() {
  if (selectResults.value.length === 0) return;
  try {
    await api.createAlbum(
      selectAlbumName.value || "未命名精选集",
      selectResults.value.map(i => i.id),
      selectResults.value.map(i => i.score)
    );
    albumSaved.value = true;
    await refreshAlbums();
    emit("refresh");
  } catch (e) {
    error.value = String(e);
  }
}

async function openAlbumDetail(a: AlbumItem) {
  const items = await api.albumMedia(a.id);
  openAlbum.value = { name: a.name, items };
}

async function removeAlbum(id: number) {
  await api.deleteAlbum(id);
  await refreshAlbums();
}
</script>

<template>
  <div class="modal-mask" @click.self="$emit('close')">
    <div class="modal" style="width: min(920px, 94vw)">
      <header>
        ✨ 智能精选
        <span class="chip">{{ status?.embCount ?? 0 }}/{{ status?.imageCount ?? 0 }} 已索引</span>
        <span class="chip" :title="status?.ollamaModels.join('、')">
          Ollama {{ status?.ollamaReady ? "已检测到" : "未安装（可选）" }}
        </span>
        <span class="spacer"></span>
        <button @click="$emit('close')">✕</button>
      </header>
      <div class="body">
        <!-- 模型状态条 -->
        <div class="ai-ready" v-if="!status?.modelsReady || status.embCount < status.imageCount">
          <p v-if="!status?.modelsReady">
            首次使用需要下载 CLIP 模型（约 600 MB，仅一次，之后全本地推理）。
          </p>
          <p v-else-if="status.embCount < status.imageCount">
            有 {{ status.imageCount - status.embCount }} 张照片还没建立语义索引。
          </p>
          <button v-if="!status?.modelsReady" class="primary" :disabled="busy" @click="downloadModels">
            下载模型
          </button>
          <button v-else class="primary" :disabled="busy" @click="buildIndex">
            建立语义索引
          </button>
        </div>
        <p v-if="progressMsg" class="ai-progress">⏳ {{ progressMsg }}</p>
        <p v-if="error" class="import-error">{{ error }}</p>

        <div class="import-tabs" style="margin-top: 10px">
          <button :class="{ on: tab === 'search' }" @click="tab = 'search'">🔍 语义搜索</button>
          <button :class="{ on: tab === 'select' }" @click="tab = 'select'">🪄 智能精选</button>
          <button :class="{ on: tab === 'albums' }" @click="tab = 'albums'; refreshAlbums()">
            📔 精选集 ({{ albums.length }})
          </button>
        </div>

        <!-- 语义搜索 -->
        <div v-if="tab === 'search'">
          <div class="form-row" style="margin-top: 12px">
            <input
              v-model="searchQuery"
              placeholder="用一句话搜照片，比如：宝宝在水里笑 / 全家福 / 户外草地"
              style="flex: 1"
              @keyup.enter="doSearch"
            />
            <button class="primary" :disabled="busy" @click="doSearch">搜索</button>
          </div>
          <p v-if="searchDone && searchResults.length === 0" class="hint">没找到相关的照片</p>
          <div class="result-grid">
            <div v-for="m in searchResults" :key="m.id" class="result-card" :title="`${m.filename} · 相似度 ${(m.score * 100).toFixed(0)}%`">
              <img v-if="m.thumb" :src="thumbUrl({ thumb: m.thumb } as any)" loading="lazy" />
              <span class="result-score">{{ (m.score * 100).toFixed(0) }}</span>
            </div>
          </div>
        </div>

        <!-- 智能精选 -->
        <div v-else-if="tab === 'select'">
          <div class="form-row" style="margin-top: 12px">
            <input
              v-model="selectPrompt"
              placeholder="例：挑 12 张宝宝笑得开心的照片"
              style="flex: 1"
              @keyup.enter="doSelect"
            />
            <select v-model="selectPool">
              <option value="all">从全部照片</option>
              <option value="keep">只从已保留</option>
            </select>
            <button class="primary" :disabled="busy || !selectPrompt" @click="doSelect">生成精选</button>
          </div>
          <p v-if="selectParsed" class="hint" style="margin: 4px 0 8px">{{ selectParsed }} · 按语义相关度 + 画面质量排序</p>
          <div class="result-grid">
            <div v-for="(m, i) in selectResults" :key="m.id" class="result-card">
              <img v-if="m.thumb" :src="thumbUrl({ thumb: m.thumb } as any)" loading="lazy" />
              <span class="rank">{{ i + 1 }}</span>
              <span class="result-score">{{ (m.score * 100).toFixed(0) }}</span>
            </div>
          </div>
          <div v-if="selectResults.length > 0" class="form-row" style="margin-top: 14px">
            <input v-model="selectAlbumName" placeholder="精选集名字" style="flex: 1" />
            <button class="primary" @click="saveAlbum">
              {{ albumSaved ? "✓ 已保存" : "存为精选集" }}
            </button>
          </div>
        </div>

        <!-- 精选集列表 -->
        <div v-else>
          <p v-if="albums.length === 0" class="hint" style="margin-top: 12px">还没有精选集——用「智能精选」生成一个吧</p>
          <div v-for="a in albums" :key="a.id" class="source-row">
            <b>{{ a.name }}</b>
            <span class="hint">{{ a.count }} 张</span>
            <span class="hint">{{ fmtTime(a.createdAt) }}</span>
            <span class="spacer"></span>
            <button style="padding: 4px 10px" @click="openAlbumDetail(a)">查看</button>
            <button style="padding: 4px 10px; color: #fca5a5" @click="removeAlbum(a.id)">删除</button>
          </div>
        </div>
      </div>

      <!-- 精选集详情 -->
      <div v-if="openAlbum" class="album-detail">
        <header>
          <b>{{ openAlbum.name }}</b> · {{ openAlbum.items.length }} 张
          <span class="spacer"></span>
          <button @click="openAlbum = null">收起</button>
        </header>
        <div class="result-grid">
          <div v-for="m in openAlbum.items" :key="m.id" class="result-card">
            <img v-if="m.thumb" :src="thumbUrl({ thumb: m.thumb } as any)" loading="lazy" />
            <span class="result-score">{{ (m.score * 100).toFixed(0) }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ai-ready {
  background: rgba(99, 102, 241, 0.1);
  border: 1px solid rgba(99, 102, 241, 0.35);
  border-radius: 10px;
  padding: 12px 16px;
  display: flex;
  align-items: center;
  gap: 14px;
  color: var(--muted);
  font-size: 13px;
}
.ai-ready p { flex: 1; line-height: 1.6; }
.ai-progress { color: #a78bfa; font-size: 12.5px; margin-top: 8px; }
.result-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 10px;
  margin-top: 12px;
  max-height: 42vh;
  overflow-y: auto;
}
.result-card {
  position: relative;
  border-radius: 10px;
  overflow: hidden;
  background: var(--bg3);
  aspect-ratio: 1;
}
.result-card img { width: 100%; height: 100%; object-fit: cover; display: block; }
.result-score {
  position: absolute;
  bottom: 5px;
  right: 5px;
  font-size: 11px;
  background: rgba(15, 17, 23, 0.75);
  color: #fff;
  padding: 1px 7px;
  border-radius: 6px;
  font-variant-numeric: tabular-nums;
}
.rank {
  position: absolute;
  top: 5px;
  left: 5px;
  font-size: 12px;
  font-weight: 700;
  background: var(--accent);
  color: #fff;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  display: grid;
  place-items: center;
}
.album-detail {
  border-top: 1px solid var(--line);
  padding: 12px 18px 16px;
  max-height: 40vh;
  overflow-y: auto;
}
.album-detail header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 0 8px;
  background: transparent;
  border: none;
}
</style>
