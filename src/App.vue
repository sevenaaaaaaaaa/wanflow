<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api, fmtBytes, type MediaItem, type ScanStatus, type Stats, type HistoryEntry, type SuggestionItem } from "./api";
import EmptyState from "./components/EmptyState.vue";
import PhotoGrid from "./components/PhotoGrid.vue";
import Lightbox from "./components/Lightbox.vue";
import DupPanel from "./components/DupPanel.vue";
import TimeModal from "./components/TimeModal.vue";
import HistoryDrawer from "./components/HistoryDrawer.vue";
import SuggestionsPanel from "./components/SuggestionsPanel.vue";
import ImportModal from "./components/ImportModal.vue";
import SmartPanel from "./components/SmartPanel.vue";
import Arena from "./components/Arena.vue";

// —— 库状态 ——
const items = ref<MediaItem[]>([]);
const total = ref(0);
const stats = ref<Stats | null>(null);
const filter = ref<"all" | "unreviewed" | "keep" | "reject" | "pending">("all");
const sort = ref<"time" | "name" | "size" | "stars" | "quality" | "added">("time");
const dir = ref<"asc" | "desc">("desc");
const dupOnly = ref(false);
const loading = ref(false);

// —— 选择与灯箱 ——
const selection = ref<Set<number>>(new Set());
const lightboxIdx = ref<number | null>(null);

// —— 扫描与分析 ——
const scan = ref<ScanStatus>({ running: false, phase: "", done: 0, total: 0, current: "" });
const analyze = ref<ScanStatus>({ running: false, phase: "", done: 0, total: 0, current: "" });
const suggestionCount = ref(0);
let scanTimer: number | undefined;
let unlisteners: (() => void)[] = [];

// —— 撤销 toast ——
const undoToast = ref<string | null>(null);
let undoTimer: number | undefined;

// —— 面板 ——
const showTimeModal = ref(false);
const showHistory = ref(false);
const history = ref<HistoryEntry[]>([]);
const dupGroup = ref<number | null>(null);
const showSuggestions = ref(false);
const showImport = ref(false);
const showSmart = ref(false);
const suggestions = ref<SuggestionItem[]>([]);
const arenaGroup = ref<number | null>(null);

const hasLibrary = computed(() => (stats.value?.total ?? 0) > 0);

async function refresh() {
  loading.value = true;
  try {
    const r = await api.listMedia({
      decision: filter.value === "all" ? undefined : filter.value,
      sort: sort.value,
      dir: dir.value,
      dupOnly: dupOnly.value || undefined,
      limit: 500,
    });
    items.value = r.items;
    total.value = r.total;
    stats.value = await api.stats();
    refreshSuggestionCount();
  } finally {
    loading.value = false;
  }
}

async function refreshSuggestionCount() {
  suggestionCount.value = (await api.getSuggestions()).length;
}

async function openSuggestions() {
  suggestions.value = await api.getSuggestions();
  showSuggestions.value = true;
}

function pickAndScan() {
  showImport.value = true;
}

function pollScan() {
  if (scanTimer) return;
  scanTimer = window.setInterval(async () => {
    scan.value = await api.getScanStatus();
    if (!scan.value.running) {
      window.clearInterval(scanTimer);
      scanTimer = undefined;
      if (scan.value.phase && scan.value.phase !== "完成") {
        showToast(`导入失败: ${scan.value.phase}`);
      }
      await refresh();
    }
  }, 400);
}

async function runAnalyze() {
  if (analyze.value.running) return;
  try {
    await api.analyzeAll();
    // 进度经事件推送（onMounted 里 listen），结束后 done 事件里刷新
  } catch (e) {
    showToast(`分析失败: ${e}`);
  }
}

async function decide(decision: "keep" | "reject" | "pending" | null, stars?: number) {
  const ids = [...selection.value];
  const cur = lightboxIdx.value;
  const target = cur !== null && ids.length === 0 ? [items.value[cur].id] : ids;
  if (target.length === 0) return;
  try {
    const label = await api.setDecision(target, decision, stars ?? null);
    showToast(`已标记 ${label}`);
    await refresh();
    // 灯箱单张模式：自动前进
    if (cur !== null && ids.length === 0 && decision) advance(1);
  } catch (e) {
    showToast(`操作失败: ${e}`);
  }
}

function advance(delta: number) {
  if (lightboxIdx.value === null) return;
  const next = lightboxIdx.value + delta;
  if (next >= 0 && next < items.value.length) lightboxIdx.value = next;
}

async function undo() {
  const label = await api.undoLast();
  showToast(label ? `已撤销：${label}` : "没有可撤销的操作");
  await refresh();
}
async function redo() {
  const label = await api.redoLast();
  showToast(label ? `已重做：${label}` : "没有可重做的操作");
  await refresh();
}

function showToast(msg: string) {
  undoToast.value = msg;
  if (undoTimer) window.clearTimeout(undoTimer);
  undoTimer = window.setTimeout(() => (undoToast.value = null), 5000);
}

async function openHistory() {
  history.value = await api.history(100);
  showHistory.value = true;
}

function onKey(e: KeyboardEvent) {
  const inLightbox = lightboxIdx.value !== null;
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
    e.preventDefault();
    e.shiftKey ? redo() : undo();
    return;
  }
  if (e.key === "Escape") {
    if (lightboxIdx.value !== null) lightboxIdx.value = null;
    else if (arenaGroup.value !== null) arenaGroup.value = null;
    else if (showSuggestions.value) showSuggestions.value = false;
    return;
  }
  if (!inLightbox) return;
  if (e.key === "ArrowLeft" || e.key === "j") { e.preventDefault(); advance(-1); }
  else if (e.key === "ArrowRight" || e.key === "k" || e.key === " ") { e.preventDefault(); advance(1); }
  else if (e.key.toLowerCase() === "p") decide("keep");
  else if (e.key.toLowerCase() === "x") decide("reject");
  else if (e.key.toLowerCase() === "u") decide("pending");
  else if (/^[0-5]$/.test(e.key)) decide(null, parseInt(e.key));
}

onMounted(async () => {
  document.addEventListener("keydown", onKey);
  unlisteners = await Promise.all([
    listen<ScanStatus>("analyze://progress", e => (analyze.value = e.payload)),
    listen("analyze://done", async () => {
      analyze.value = { running: false, phase: "完成", done: 0, total: 0, current: "" };
      await refresh();
      showToast("质量分析完成，初筛建议已生成");
    }),
    listen<string>("analyze://error", async e => {
      analyze.value = { running: false, phase: e.payload, done: 0, total: 0, current: "" };
      showToast(`分析失败: ${e.payload}`);
    }),
  ]);
  await refresh();
  const s = await api.getScanStatus();
  if (s.running) { scan.value = s; pollScan(); }
});
onUnmounted(() => {
  document.removeEventListener("keydown", onKey);
  unlisteners.forEach(u => u());
  if (scanTimer) window.clearInterval(scanTimer);
});

const progressPct = computed(() =>
  scan.value.total > 0 ? Math.round((scan.value.done / scan.value.total) * 100) : 0
);
</script>

<template>
  <div class="app-shell">
    <!-- 顶栏 -->
    <div class="topbar" v-if="hasLibrary || scan.running">
      <span class="brand">Wan<em>Flow</em></span>
      <span class="chip">{{ stats?.total ?? 0 }} 张 · {{ fmtBytes(stats?.bytes ?? 0) }}</span>
      <button class="primary" @click="showImport = true" :disabled="scan.running">
        {{ scan.running ? "导入中…" : "导入" }}
      </button>

      <span class="spacer"></span>

      <select v-model="filter" @change="refresh">
        <option value="all">全部</option>
        <option value="unreviewed">未处理</option>
        <option value="keep">已保留</option>
        <option value="pending">待定</option>
        <option value="reject">已淘汰</option>
      </select>
      <select v-model="sort" @change="refresh">
        <option value="time">拍摄时间</option>
        <option value="added">导入顺序</option>
        <option value="name">文件名</option>
        <option value="size">大小</option>
        <option value="stars">星级</option>
        <option value="quality">质量最差优先</option>
      </select>
      <button @click="dir = dir === 'asc' ? 'desc' : 'asc'; refresh()" :title="dir === 'asc' ? '当前：正序' : '当前：倒序'">
        {{ dir === "asc" ? "↑" : "↓" }}
      </button>
      <label class="hint" style="display:flex;align-items:center;gap:4px">
        <input type="checkbox" v-model="dupOnly" @change="refresh" /> 仅重复组
      </label>

      <span class="spacer"></span>

      <button @click="showSmart = true">✨ 智能精选</button>
      <button @click="runAnalyze" :disabled="analyze.running">
        {{ analyze.running ? "分析中…" : "质量分析" }}
      </button>
      <button @click="openSuggestions">
        智能初筛<span v-if="suggestionCount > 0" class="count-badge">{{ suggestionCount }}</span>
      </button>
      <button @click="undo" title="⌘Z">撤销</button>
      <button @click="redo" title="⇧⌘Z">重做</button>
      <button @click="openHistory">历史</button>
    </div>

    <!-- 批量操作条 -->
    <div class="topbar" v-if="selection.size > 0" style="background: var(--bg3)">
      <div class="batchbar">
        <b>{{ selection.size }} 张选中</b>
        <button class="primary" @click="decide('keep')">保留 (P)</button>
        <button @click="decide('pending')">待定 (U)</button>
        <button style="color: #fca5a5" @click="decide('reject')">淘汰 (X)</button>
        <span style="color: var(--muted)">|</span>
        <button v-for="s in 5" :key="s" @click="decide(null, s)">★{{ s }}</button>
        <span style="color: var(--muted)">|</span>
        <button @click="showTimeModal = true">修正时间…</button>
        <button @click="selection.clear()">取消选择</button>
      </div>
    </div>

    <!-- 扫描进度 -->
    <div class="scanbar" v-if="scan.running || progressPct > 0">
      <div :style="{ width: scan.running ? Math.max(progressPct, 2) + '%' : '100%' }"></div>
    </div>
    <div class="hint" v-if="scan.running" style="padding: 4px 16px">
      {{ scan.phase }} · {{ scan.done }}/{{ scan.total }} · {{ scan.current }}
    </div>
    <div class="scanbar" v-if="analyze.running">
      <div style="width: 30%" class="analyze-anim"></div>
    </div>
    <div class="hint" v-if="analyze.running" style="padding: 4px 16px; color: #a78bfa">
      ✨ {{ analyze.phase }} · {{ analyze.current }}
    </div>

    <!-- 主区域 -->
    <EmptyState v-if="!hasLibrary && !scan.running" @pick="pickAndScan" />
    <PhotoGrid
      v-else
      :items="items"
      :total="total"
      :loading="loading"
      :selection="selection"
      @toggle="id => { selection.has(id) ? selection.delete(id) : selection.add(id); }"
      @open="i => (lightboxIdx = i)"
      @dup="g => (dupGroup = g)"
      @arena="g => (arenaGroup = g)"
    />

    <!-- 底部状态栏 -->
    <div class="statusbar" v-if="hasLibrary">
      <span>保留 <b style="color: var(--keep)">{{ stats?.keeps ?? 0 }}</b></span>
      <span>待定 {{ stats?.pendings ?? 0 }}</span>
      <span>淘汰 <b style="color: var(--reject)">{{ stats?.rejects ?? 0 }}</b></span>
      <span>未处理 {{ stats?.unreviewed ?? 0 }}</span>
      <span>重复组 {{ stats?.dupGroups ?? 0 }}</span>
      <span class="spacer"></span>
      <span class="hint">
        <kbd>点击</kbd> 选择 · <kbd>双击</kbd> 放大 · <kbd>P</kbd> 保留 · <kbd>X</kbd> 淘汰 ·
        <kbd>0-5</kbd> 星级 · <kbd>⌘Z</kbd> 撤销
      </span>
    </div>

    <!-- 灯箱 -->
    <Lightbox
      v-if="lightboxIdx !== null"
      :item="items[lightboxIdx]"
      :pos="lightboxIdx + 1"
      :count="items.length"
      @close="lightboxIdx = null"
      @prev="advance(-1)"
      @next="advance(1)"
      @decide="d => decide(d)"
      @stars="s => decide(null, s)"
      @dup="g => (dupGroup = g)"
      @arena="g => ((arenaGroup = g), (lightboxIdx = null))"
    />

    <!-- 重复组面板 -->
    <DupPanel
      v-if="dupGroup !== null"
      :group="dupGroup"
      @close="dupGroup = null"
      @decide="d => decide(d)"
      @refresh="refresh"
    />

    <!-- 批量时间操作台 -->
    <TimeModal
      v-if="showTimeModal"
      :ids="[...selection]"
      @close="showTimeModal = false"
      @applied="async label => { showToast(label); await refresh(); }"
    />

    <!-- 历史抽屉 -->
    <HistoryDrawer
      v-if="showHistory"
      :entries="history"
      @close="showHistory = false"
      @undo="undo"
      @redo="redo"
    />

    <!-- 智能精选 -->
    <SmartPanel
      v-if="showSmart"
      @close="showSmart = false"
      @refresh="refresh"
    />

    <!-- 导入中心 -->
    <ImportModal
      v-if="showImport"
      @close="showImport = false"
      @started="() => { showImport = false; pollScan(); }"
    />

    <!-- 智能初筛面板 -->
    <SuggestionsPanel
      v-if="showSuggestions"
      :items="suggestions"
      @close="showSuggestions = false"
      @applied="async label => { showToast(label); showSuggestions = false; await refresh(); }"
      @refresh="refresh"
    />

    <!-- 擂台 -->
    <Arena
      v-if="arenaGroup !== null"
      :group="arenaGroup"
      @close="arenaGroup = null"
      @refresh="refresh"
    />

    <!-- 撤销 toast -->
    <div class="undo-toast" v-if="undoToast">
      <span>{{ undoToast }}</span>
      <button @click="undo()">撤销</button>
      <button style="color: var(--muted)" @click="undoToast = null">✕</button>
    </div>
  </div>
</template>
