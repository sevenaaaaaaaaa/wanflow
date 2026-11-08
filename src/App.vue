<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import {
  api, fmtBytes, type MediaItem, type ScanStatus, type Stats, type HistoryEntry, type SuggestionItem,
} from "./api";
import EmptyState from "./components/EmptyState.vue";
import PhotoGrid from "./components/PhotoGrid.vue";
import Lightbox from "./components/Lightbox.vue";
import DupPanel from "./components/DupPanel.vue";
import TimeModal from "./components/TimeModal.vue";
import HistoryDrawer from "./components/HistoryDrawer.vue";
import SuggestionsPanel from "./components/SuggestionsPanel.vue";
import ImportModal from "./components/ImportModal.vue";
import SmartPanel from "./components/SmartPanel.vue";
import FamilyPanel from "./components/FamilyPanel.vue";
import Arena from "./components/Arena.vue";
import ExportModal from "./components/ExportModal.vue";
import CommandPalette from "./components/CommandPalette.vue";
import Storyline from "./components/Storyline.vue";
import type { Cmd } from "./types";
import Icon from "./components/Icon.vue";

// —— 库状态 ——
const items = ref<MediaItem[]>([]);
const total = ref(0);
const stats = ref<Stats | null>(null);
type Filter = "all" | "unreviewed" | "keep" | "reject" | "pending";
const filter = ref<Filter>("all");
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
const undoSteps = ref(1);
const redoSteps = ref(1);
let undoTimer: number | undefined;

// —— 浮层 ——
const showTimeModal = ref(false);
const showHistory = ref(false);
const history = ref<HistoryEntry[]>([]);
const dupGroup = ref<number | null>(null);
const showSuggestions = ref(false);
const showImport = ref(false);
const showSmart = ref(false);
const showFamily = ref(false);
const suggestions = ref<SuggestionItem[]>([]);
const arenaGroup = ref<number | null>(null);
const showExport = ref(false);
const importTab = ref<"folder" | "photos" | "webdav" | "qq" | "manage">("folder");
const storyRef = ref<{ startTour: () => void } | null>(null);

// —— 命令面板 ——
const paletteOpen = ref(false);

// —— 视图：故事线（默认）/ 网格 ——
type View = "story" | "grid";
const view = ref<View>("story");
const revision = ref(0);
function setView(v: View) {
  view.value = v;
  localStorage.setItem("wanflow-view", v);
}

// —— 主题 ——
const theme = ref<"dark" | "light">("light");
function applyTheme() {
  document.documentElement.dataset.theme = theme.value;
  localStorage.setItem("wanflow-theme", theme.value);
}
function toggleTheme() {
  theme.value = theme.value === "dark" ? "light" : "dark";
  applyTheme();
}

const hasLibrary = computed(() => (stats.value?.total ?? 0) > 0);
const selectedIds = computed(() => [...selection.value]);
const focusHint = computed(() => (selection.value.size > 0 ? `${selection.value.size} 张选中` : ""));

const FILTERS: { key: Filter; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "unreviewed", label: "未处理" },
  { key: "keep", label: "保留" },
  { key: "pending", label: "待定" },
  { key: "reject", label: "淘汰" },
];

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
  try { suggestionCount.value = (await api.getSuggestions()).length; } catch { /* 忽略 */ }
}

async function openSuggestions() {
  suggestions.value = await api.getSuggestions();
  showSuggestions.value = true;
}

function pickAndScan(kind: "folder" | "photos" = "folder") {
  importTab.value = kind;
  showImport.value = true;
}

// 单一动作入口：还有没挑的就回故事线，挑完了就去精选
function continueReview() {
  if ((stats.value?.unreviewed ?? 0) > 0) {
    setView("story");
    refresh();
  } else {
    showSmart.value = true;
  }
}

function startTour() {
  if (!hasLibrary.value) return;
  if (view.value !== "story") setView("story");
  nextTick(() => storyRef.value?.startTour());
}

function pollScan() {
  if (scanTimer) return;
  scanTimer = window.setInterval(async () => {
    scan.value = await api.getScanStatus();
    if (!scan.value.running) {
      window.clearInterval(scanTimer);
      scanTimer = undefined;
      if (scan.value.phase && scan.value.phase !== "完成") showToast(`导入失败: ${scan.value.phase}`);
      await refresh();
    }
  }, 400);
}

async function runAnalyze() {
  if (analyze.value.running) return;
  try {
    await api.analyzeAll();
  } catch (e) {
    showToast(`分析失败: ${e}`);
  }
}

function setFilter(f: Filter) {
  filter.value = f;
  selection.value = new Set();
  refresh();
}

async function decide(decision: "keep" | "reject" | "pending" | null, stars?: number) {
  const ids = [...selection.value];
  const cur = lightboxIdx.value;
  const target = cur !== null && ids.length === 0 ? [items.value[cur].id] : ids;
  if (target.length === 0) return;
  await decideIds(target, decision, stars);
  if (cur !== null && ids.length === 0 && decision) advance(1);
}

async function decideIds(ids: number[], decision: "keep" | "reject" | "pending" | null, stars?: number) {
  if (ids.length === 0) return;
  try {
    const label = await api.setDecision(ids, decision, stars ?? null);
    showToast(`已标记 ${label}`);
    await refresh();
  } catch (e) {
    showToast(`操作失败: ${e}`);
  }
}

async function star(ids: number[], s: number) {
  if (ids.length === 0) return;
  try {
    await api.setDecision(ids, null, s);
    showToast(`已加星 ★${s}`);
    await refresh();
  } catch (e) {
    showToast(`操作失败: ${e}`);
  }
}

function advance(delta: number) {
  if (lightboxIdx.value === null) return;
  const next = lightboxIdx.value + delta;
  if (next >= 0 && next < items.value.length) lightboxIdx.value = next;
}

// 一次显影是两个批次（保留 + 其余转待定），撤销/重做要按整组走
async function undo(steps = undoSteps.value) {
  let label: string | null = null;
  for (let i = 0; i < steps; i++) label = await api.undoLast();
  revision.value++;
  undoSteps.value = 1;
  redoSteps.value = steps;
  announce(label ? `已撤销：${label}` : "没有可撤销的操作");
  await refresh();
}
async function redo(steps = redoSteps.value) {
  let label: string | null = null;
  for (let i = 0; i < steps; i++) label = await api.redoLast();
  revision.value++;
  undoSteps.value = steps;
  redoSteps.value = 1;
  announce(label ? `已重做：${label}` : "没有可重做的操作");
  await refresh();
}

function announce(msg: string) {
  undoToast.value = msg;
  if (undoTimer) window.clearTimeout(undoTimer);
  undoTimer = window.setTimeout(() => (undoToast.value = null), 5000);
}

function showToast(msg: string, steps = 1) {
  undoSteps.value = steps;
  redoSteps.value = 1;
  announce(msg);
}

function onDeveloped(p: { label: string; steps: number }) {
  showToast(p.label, p.steps);
  refresh();
}

async function openHistory() {
  history.value = await api.history(100);
  showHistory.value = true;
}

// —— 检查更新（GitHub Release）——
const checkingUpdate = ref(false);
async function checkForUpdates() {
  if (checkingUpdate.value) return;
  checkingUpdate.value = true;
  try {
    const r = await api.checkUpdate();
    if (r.error) {
      showToast(`检查更新失败：${r.error}`);
    } else if (r.newer) {
      if (r.url) {
        try { await navigator.clipboard.writeText(r.url); } catch { /* 剪贴板不可用则只提示 */ }
        showToast(`发现新版本 v${r.latest}（当前 v${r.current}），下载地址已复制到剪贴板`);
      } else {
        showToast(`发现新版本 v${r.latest}（当前 v${r.current}）`);
      }
    } else {
      showToast(`已是最新版本 v${r.current}`);
    }
  } finally {
    checkingUpdate.value = false;
  }
}

function toggleId(id: number) {
  const next = new Set(selection.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  selection.value = next;
}
function selectAll() {
  selection.value = new Set(items.value.map((m) => m.id));
}
function clearSelection() {
  selection.value = new Set();
}
function addRange(ids: number[]) {
  if (ids.length === 0) { selection.value = new Set(); return; }
  const next = new Set(selection.value);
  ids.forEach((id) => next.add(id));
  selection.value = next;
}

function closeTop() {
  if (paletteOpen.value) return (paletteOpen.value = false);
  if (lightboxIdx.value !== null) return (lightboxIdx.value = null);
  if (dupGroup.value !== null) return (dupGroup.value = null);
  if (arenaGroup.value !== null) return (arenaGroup.value = null);
  if (showSuggestions.value) return (showSuggestions.value = false);
  if (showSmart.value) return (showSmart.value = false);
  if (showExport.value) return (showExport.value = false);
  if (showTimeModal.value) return (showTimeModal.value = false);
  if (showHistory.value) return (showHistory.value = false);
  if (showImport.value) return (showImport.value = false);
  if (selection.value.size > 0) return clearSelection();
}

function onKey(e: KeyboardEvent) {
  const el = e.target as HTMLElement | null;
  const typing = !!el && /^(input|textarea|select)$/i.test(el.tagName);

  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    paletteOpen.value = !paletteOpen.value;
    return;
  }
  if ((e.metaKey || e.ctrlKey) && (e.key === "1" || e.key === "2")) {
    e.preventDefault();
    setView(e.key === "1" ? "story" : "grid");
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
    if (typing) return;
    e.preventDefault();
    e.shiftKey ? redo() : undo();
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a" && !typing && lightboxIdx.value === null) {
    e.preventDefault();
    selectAll();
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "i" && !typing) {
    e.preventDefault();
    showImport.value = true;
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "e" && !typing) {
    e.preventDefault();
    showExport.value = true;
    return;
  }
  if (e.key === "Escape") { closeTop(); return; }
  if (typing) return;

  const inLightbox = lightboxIdx.value !== null;
  const anyOverlay =
    paletteOpen.value || dupGroup.value !== null || arenaGroup.value !== null ||
    showSuggestions.value || showSmart.value || showImport.value || showExport.value ||
    showTimeModal.value || showHistory.value;

  if (e.key === "/" && !inLightbox && !anyOverlay) {
    e.preventDefault();
    paletteOpen.value = true;
    return;
  }
  if (e.key === "?" && !inLightbox && !anyOverlay) {
    e.preventDefault();
    startTour();
    return;
  }
  if (!inLightbox || anyOverlay) return;

  if (e.key === "ArrowLeft" || e.key === "j" || e.key === "ArrowUp") { e.preventDefault(); advance(-1); }
  else if (e.key === "ArrowRight" || e.key === "k" || e.key === "ArrowDown" || e.key === " ") { e.preventDefault(); advance(1); }
  else if (e.key.toLowerCase() === "p") decide("keep");
  else if (e.key.toLowerCase() === "x") decide("reject");
  else if (e.key.toLowerCase() === "u") decide("pending");
  else if (/^[0-5]$/.test(e.key)) decide(null, parseInt(e.key, 10));
}

// —— 命令面板：与动作栏同源 ——
const commands = computed<Cmd[]>(() => {
  const sel = selectedIds.value;
  const out: Cmd[] = [
    { id: "continue", group: "挑图", label: stats.value?.unreviewed ? `继续整理（还剩 ${stats.value.unreviewed} 张）` : "挑完了，去精选", run: continueReview },
    { id: "keep", group: "挑图", label: "保留", kbd: "P", hint: sel.length ? `${sel.length} 张` : "", run: () => decide("keep") },
    { id: "pending", group: "挑图", label: "待定", kbd: "U", run: () => decide("pending") },
    { id: "reject", group: "挑图", label: "淘汰", kbd: "X", run: () => decide("reject") },
    ...[1, 2, 3, 4, 5].map<Cmd>((s) => ({ id: `star${s}`, group: "挑图", label: `加星 ★${s}`, kbd: String(s), run: () => decide(null, s) })),
    { id: "all", group: "挑图", label: "全选当前列表", kbd: "⌘A", run: selectAll },
    { id: "none", group: "挑图", label: "清空选择", kbd: "esc", run: clearSelection },
    { id: "import", group: "导入与分析", label: "导入照片/视频…", kbd: "⌘I", run: () => (showImport.value = true) },
    { id: "analyze", group: "导入与分析", label: analyze.value.running ? "质量分析（进行中…）" : "重新做质量分析", run: runAnalyze },
    { id: "suggest", group: "导入与分析", label: `智能初筛${suggestionCount.value ? `（${suggestionCount.value} 条建议）` : ""}`, run: openSuggestions },
    { id: "smart", group: "导入与分析", label: "智能精选…", run: () => (showSmart.value = true) },
    { id: "family", group: "导入与分析", label: "家人与面孔…", run: () => (showFamily.value = true) },
    { id: "time", group: "整理", label: "修正拍摄时间…", hint: sel.length ? `${sel.length} 张` : "", run: () => (showTimeModal.value = true) },
    { id: "dup", group: "整理", label: dupOnly.value ? "显示全部（不限重复组）" : "只看重复组", run: () => { dupOnly.value = !dupOnly.value; refresh(); } },
    { id: "export", group: "输出", label: "导出精选集…", kbd: "⌘E", hint: sel.length ? `${sel.length} 张选中` : "按范围", run: () => (showExport.value = true) },
    { id: "undo", group: "输出", label: "撤销上一次操作", kbd: "⌘Z", run: undo },
    { id: "redo", group: "输出", label: "重做", kbd: "⇧⌘Z", run: redo },
    { id: "history", group: "输出", label: "操作历史…", run: openHistory },
    { id: "theme", group: "界面", label: theme.value === "dark" ? "切到亮色" : "切到暗色", run: toggleTheme },
    { id: "tour", group: "视图", label: "30 秒上手：一天一页", kbd: "?", run: startTour },
    { id: "view-story", group: "视图", label: "故事线：一天一页，定量显影", kbd: "⌘1", run: () => setView("story") },
    { id: "view-grid", group: "视图", label: "网格：逐张浏览", kbd: "⌘2", run: () => setView("grid") },
    ...FILTERS.map<Cmd>((f) => ({ id: `f-${f.key}`, group: "视图", label: `筛选：${f.label}`, run: () => setFilter(f.key) })),
    { id: "update", group: "帮助", label: "检查更新…", run: checkForUpdates },
  ];
  return out;
});

onMounted(async () => {
  const saved = localStorage.getItem("wanflow-theme");
  if (saved === "light" || saved === "dark") theme.value = saved;
  const savedView = localStorage.getItem("wanflow-view");
  if (savedView === "story" || savedView === "grid") view.value = savedView;
  applyTheme();
  // 首屏短暂露出工具条：让它被看见一次，然后自己隐入纸面
  window.setTimeout(() => { if (!topbarTouched) topbarHovered.value = false; }, 1800);

  document.addEventListener("keydown", onKey);
  try {
    unlisteners = await Promise.all([
      listen<ScanStatus>("analyze://progress", (e) => (analyze.value = e.payload)),
      listen("analyze://done", async () => {
        analyze.value = { running: false, phase: "完成", done: 0, total: 0, current: "" };
        await refresh();
        showToast("质量分析完成，初筛建议已生成");
      }),
      listen<string>("analyze://error", async (e) => {
        analyze.value = { running: false, phase: e.payload, done: 0, total: 0, current: "" };
        showToast(`分析失败: ${e.payload}`);
      }),
      listen("clip://done", async () => {
        await refresh();
        showToast("AI 任务完成");
      }),
      listen<string>("clip://error", async (e) => {
        showToast(`AI 任务失败: ${e.payload}`);
      }),
    ]);
  } catch {
    // 浏览器预览（无 Tauri 事件总线）时静默降级，不影响页面与演示数据。
  }
  await refresh();
  const s = await api.getScanStatus();
  if (s.running) { scan.value = s; pollScan(); }
});
onUnmounted(() => {
  if (topbarTimer) window.clearTimeout(topbarTimer);
  document.removeEventListener("keydown", onKey);
  unlisteners.forEach((u) => u());
  if (scanTimer) window.clearInterval(scanTimer);
});

const progressPct = computed(() =>
  scan.value.total > 0 ? Math.round((scan.value.done / scan.value.total) * 100) : 0,
);

// —— 顶栏：默认隐入纸面；顶边悬停 / 网格视图 / 任务进行中时浮出 ——
const topbarHovered = ref(true);
let topbarTimer: number | null = null;
let topbarTouched = false;
const topbarBusy = computed(() => scan.running || analyze.running || progressPct.value > 0);
const topbarShown = computed(() => topbarHovered.value || topbarBusy.value || view.value === "grid");
function topbarEnter() {
  topbarTouched = true;
  if (topbarTimer) { window.clearTimeout(topbarTimer); topbarTimer = null; }
  topbarHovered.value = true;
}
function topbarLeave() {
  if (topbarTimer) window.clearTimeout(topbarTimer);
  topbarTimer = window.setTimeout(() => { topbarHovered.value = false; }, 500);
}

</script>

<template>
  <div class="app-shell">
    <!-- 顶栏：默认隐入纸面；鼠标碰到顶边 / 切网格 / 任务进行时浮出 -->
    <div class="topbar-hot" v-if="hasLibrary || scan.running" @mouseenter="topbarEnter"></div>
    <div
      class="topbar"
      :class="{ 'is-shown': topbarShown }"
      v-if="hasLibrary || scan.running"
      @mouseenter="topbarEnter"
      @mouseleave="topbarLeave"
    >
      <span class="brand">Wan<em>Flow</em></span>
      <span class="chip">{{ stats?.total ?? 0 }} 张 · {{ fmtBytes(stats?.bytes ?? 0) }}</span>

      <div class="seg" role="tablist" aria-label="视图">
        <button :class="{ on: view === 'story' }" @click="setView('story')">故事线</button>
        <button :class="{ on: view === 'grid' }" @click="setView('grid')">网格</button>
      </div>

      <div class="seg" v-if="view === 'grid'" role="tablist" aria-label="筛选">
        <button
          v-for="f in FILTERS"
          :key="f.key"
          :class="{ on: filter === f.key }"
          @click="setFilter(f.key)"
        >{{ f.label }}</button>
      </div>

      <span class="spacer"></span>

      <label v-if="view === 'grid'" class="hint" style="display: flex; align-items: center; gap: 6px">
        <input type="checkbox" v-model="dupOnly" @change="refresh" /> 仅重复组
      </label>

      <select v-if="view === 'grid'" v-model="sort" @change="refresh" aria-label="排序">
        <option value="time">拍摄时间</option>
        <option value="added">导入顺序</option>
        <option value="name">文件名</option>
        <option value="size">大小</option>
        <option value="stars">星级</option>
        <option value="quality">质量最差优先</option>
      </select>
      <button
        v-if="view === 'grid'"
        class="icon-btn"
        :title="dir === 'asc' ? '正序（点击切倒序）' : '倒序（点击切正序）'"
        @click="dir = dir === 'asc' ? 'desc' : 'asc'; refresh()"
      >
        <Icon :name="dir === 'asc' ? 'arrowUp' : 'arrowDown'" /></button>

      <button class="icon-btn" title="怎么用（?）" @click="startTour">
        <Icon name="help" />
      </button>
      <button class="ghost icon-only" @click="runAnalyze" :disabled="analyze.running" :title="analyze.running ? '质量分析进行中…' : '质量分析（逐个分析全库）'">
        <Icon name="flame" />
      </button>
      <button class="ghost icon-only" @click="showSmart = true" title="智能精选：语义搜索 · 智能挑图">
        <Icon name="sparkle" />
      </button>
      <button class="ghost icon-only" @click="showFamily = true" title="家人：面孔聚类与按人筛选">
        <Icon name="users" />
      </button>
      <button class="primary icon-only" @click="showImport = true" :disabled="scan.running" :title="scan.running ? '导入中…' : '导入照片/视频（⌘I）'">
        <Icon name="folder" />
      </button>
      <button class="icon-btn" title="命令面板 ⌘K" @click="paletteOpen = true">
        <Icon name="command" />
      </button>
    </div>

    <!-- 扫描 / 分析进度：1px 发丝 -->
    <div class="scanbar" v-if="scan.running || progressPct > 0">
      <div :style="{ width: scan.running ? Math.max(progressPct, 2) + '%' : '100%' }"></div>
    </div>
    <div class="hint" v-if="scan.running" style="padding: 5px 16px; font-family: var(--font-mono); font-size: var(--fs-1)">
      {{ scan.phase }} · {{ scan.done }}/{{ scan.total }} · {{ scan.current }}
    </div>
    <div class="scanbar" v-if="analyze.running">
      <div style="width: 30%" class="analyze-anim"></div>
    </div>
    <div class="hint" v-if="analyze.running" style="padding: 5px 16px; font-size: var(--fs-1)">
      {{ analyze.phase }} · {{ analyze.current }}
    </div>

    <!-- 主区域 -->
    <EmptyState v-if="!hasLibrary && !scan.running" @pick="pickAndScan" @check-update="checkForUpdates" />
    <Storyline
      v-else-if="view === 'story'"
      ref="storyRef"
      :revision="revision"
      :unreviewed="stats?.unreviewed ?? 0"
      @changed="onDeveloped"
      @toast="showToast"
      @grid="setView('grid')"
      @export="showExport = true"
      @continue="continueReview"
    />
    <PhotoGrid
      v-else
      :items="items"
      :total="total"
      :loading="loading"
      :selection="selection"
      @toggle="toggleId"
      @range="addRange"
      @open="(i: number) => (lightboxIdx = i)"
      @decide="(ids, d) => decideIds(ids, d)"
      @star="star"
      @dup="(g: number) => (dupGroup = g)"
      @arena="(g: number) => (arenaGroup = g)"
    />

    <!-- 底部 Dock：统计 / 选区动作栏 + 命令入口 -->
    <div class="dock" v-if="hasLibrary && view === 'grid'">
      <template v-if="selection.size > 0">
        <div class="actionbar">
          <span class="count">{{ focusHint }}</span>
          <button class="primary" @click="decide('keep')"><Icon name="check" :size="13" /> 保留</button>
          <button @click="decide('pending')">待定</button>
          <button class="danger" @click="decide('reject')"><Icon name="x" :size="13" /> 淘汰</button>
          <span class="sep"></span>
          <div class="starpick" aria-label="星级">
            <button v-for="s in 5" :key="s" :title="`${s} 星`" @click="decide(null, s)">★</button>
          </div>
          <span class="sep"></span>
          <button @click="showTimeModal = true"><Icon name="time" :size="13" /> 修正时间</button>
          <button @click="showExport = true"><Icon name="export" :size="13" /> 导出</button>
          <button class="ghost" @click="clearSelection">取消</button>
        </div>
      </template>
      <template v-else>
        <div class="stats">
          <span class="k">保留 <b>{{ stats?.keeps ?? 0 }}</b></span>
          <span class="p">待定 <b>{{ stats?.pendings ?? 0 }}</b></span>
          <span class="r">淘汰 <b>{{ stats?.rejects ?? 0 }}</b></span>
          <span>未处理 <b>{{ stats?.unreviewed ?? 0 }}</b></span>
          <span v-if="(stats?.dupGroups ?? 0) > 0">重复组 <b>{{ stats?.dupGroups ?? 0 }}</b></span>
        </div>
        <span class="spacer"></span>
        <div class="look">
          <span><kbd>↑↓←→</kbd> 移动</span>
          <span><kbd>空格</kbd> 选</span>
          <span><kbd>↵</kbd> 放大</span>
          <span><kbd>P</kbd>/<kbd>X</kbd> 留/弃</span>
        </div>
      </template>

      <button class="cmdbtn" @click="paletteOpen = true" title="命令面板">
        <Icon name="search" :size="14" />
        <span>命令</span>
        <kbd>⌘K</kbd>
      </button>
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
      @decide="(d) => decide(d)"
      @stars="(s) => decide(null, s)"
      @dup="(g) => (dupGroup = g)"
      @arena="(g) => ((arenaGroup = g), (lightboxIdx = null))"
    />

    <!-- 命令面板 -->
    <CommandPalette :open="paletteOpen" :commands="commands" @close="paletteOpen = false" />

    <!-- 重复组面板 -->
    <DupPanel v-if="dupGroup !== null" :group="dupGroup" @close="dupGroup = null" @decide="(d) => decide(d)" @refresh="refresh" />

    <!-- 批量时间操作台 -->
    <TimeModal
      v-if="showTimeModal"
      :ids="selectedIds"
      @close="showTimeModal = false"
      @applied="async (label) => { showToast(label); await refresh(); }"
    />

    <!-- 历史抽屉 -->
    <HistoryDrawer v-if="showHistory" :entries="history" @close="showHistory = false" @undo="undo" @redo="redo" />

    <!-- 智能精选 -->
    <SmartPanel v-if="showSmart" @close="showSmart = false" @refresh="refresh" />

    <!-- 家人面板 -->
    <FamilyPanel
      v-if="showFamily"
      @close="showFamily = false"
      @refresh="refresh"
    />

    <!-- 导入中心 -->
    <ImportModal
      v-if="showImport"
      :initial-tab="importTab"
      @close="showImport = false"
      @started="() => { showImport = false; pollScan(); }"
    />

    <!-- 智能初筛 -->
    <SuggestionsPanel
      v-if="showSuggestions"
      :items="suggestions"
      @close="showSuggestions = false"
      @applied="async (label) => { showToast(label); showSuggestions = false; await refresh(); }"
      @refresh="refresh"
    />

    <!-- 导出精选集 -->
    <ExportModal
      v-if="showExport"
      :ids="selectedIds"
      @close="showExport = false"
      @done="(s) => { showToast(`已导出 ${s.copied} 个文件到 ${s.destDir}`); refresh(); }"
    />

    <!-- 擂台 -->
    <Arena v-if="arenaGroup !== null" :group="arenaGroup" @close="arenaGroup = null" @refresh="refresh" />

    <!-- 撤销 toast -->
    <div class="undo-toast" :class="{ 'toast-top': view === 'story' }" v-if="undoToast">
      <span>{{ undoToast }}</span>
      <button @click="undo(undoSteps)">撤销</button>
      <button style="color: var(--text-3)" @click="undoToast = null">✕</button>
    </div>
  </div>
</template>