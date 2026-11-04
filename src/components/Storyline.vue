<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { api, thumbUrl, type MediaItem, type Stats } from "../api";
import Icon from "./Icon.vue";

const props = defineProps<{ revision: number }>();
const emit = defineEmits<{
  changed: [payload: { label: string; steps: number }];
  toast: [text: string];
  grid: [];
  export: [];
}>();

interface Day {
  key: string;
  ts: number;
  items: MediaItem[];
}

const MAX_BUDGET = 6;
const WEEK = "日一二三四五六";

const items = ref<MediaItem[]>([]);
const stats = ref<Stats | null>(null);
const loading = ref(true);
const error = ref<string | null>(null);
const activeIdx = ref(0);
const busy = ref(false);
const flashing = ref(false);
const leaving = ref(false);
const root = ref<HTMLElement | null>(null);
const stripEl = ref<HTMLElement | null>(null);
const ticksEl = ref<HTMLElement | null>(null);
const dragging = ref(false);
const dragOver = ref(false);
const dragId = ref<number | null>(null);

const budget = ref<Record<string, number>>({});
const pinned = ref<Record<string, number[]>>({});
const banned = ref<Record<string, number[]>>({});

interface Page {
  key: string;
  ts: number;
  count: number;
  thumbs: string[];
}
const kept = ref<MediaItem[]>([]);
// 只把「这一轮显影过的日子」收进精选册；保留名单本身从 kept 推导，撤销/重做才会同步
const DAYS_KEY = "wanflow-developed-days";
const developedDays = ref<string[]>([]);
try {
  const raw = localStorage.getItem(DAYS_KEY);
  if (raw) developedDays.value = JSON.parse(raw);
} catch { /* 本地存储不可用就不记忆 */ }
function rememberDay(key: string) {
  if (developedDays.value.includes(key)) return;
  developedDays.value = [...developedDays.value, key];
  try { localStorage.setItem(DAYS_KEY, JSON.stringify(developedDays.value)); } catch { /* 忽略 */ }
}
const developed = computed<Page[]>(() => {
  const show = new Set(developedDays.value);
  const map = new Map<string, MediaItem[]>();
  for (const m of kept.value) {
    const key = dayKey(m.effTime);
    const list = map.get(key);
    if (list) list.push(m);
    else map.set(key, [m]);
  }
  const out: Page[] = [];
  for (const [key, list] of map) {
    if (!show.has(key)) continue;
    list.sort((a, b) => a.effTime - b.effTime);
    out.push({ key, ts: list[0].effTime, count: list.length, thumbs: list.slice(0, 4).map((m) => thumbUrl(m)) });
  }
  out.sort((a, b) => b.ts - a.ts);
  return out;
});

const pad2 = (n: number) => (n < 10 ? "0" + n : String(n));
function parts(ts: number) {
  const d = new Date(ts * 1000);
  return { y: d.getFullYear(), m: d.getMonth() + 1, d: d.getDate(), w: "周" + WEEK[d.getDay()] };
}
function dayKey(ts: number) {
  const p = parts(ts);
  return p.y + "-" + pad2(p.m) + "-" + pad2(p.d);
}
function monthLabel(ts: number) {
  const p = parts(ts);
  return p.y + " 年 " + p.m + " 月";
}

const days = computed<Day[]>(() => {
  const map = new Map<string, MediaItem[]>();
  for (const m of items.value) {
    const key = dayKey(m.effTime);
    const list = map.get(key);
    if (list) list.push(m);
    else map.set(key, [m]);
  }
  const out: Day[] = [];
  for (const [key, list] of map) {
    list.sort((a, b) => a.effTime - b.effTime);
    out.push({ key, ts: list[0].effTime, items: list });
  }
  out.sort((a, b) => b.ts - a.ts);
  return out;
});

const months = computed(() => {
  const out: { label: string; days: Day[] }[] = [];
  for (const d of days.value) {
    const label = monthLabel(d.ts);
    const last = out[out.length - 1];
    if (last && last.label === label) last.days.push(d);
    else out.push({ label, days: [d] });
  }
  return out;
});

const active = computed<Day | null>(() => days.value[activeIdx.value] ?? null);
const activeParts = computed(() => (active.value ? parts(active.value.ts) : null));

function defaultBudget(n: number) {
  return Math.min(MAX_BUDGET, Math.max(1, Math.round(Math.sqrt(n))));
}
function budgetFor(day: Day) {
  const b = budget.value[day.key];
  const max = Math.min(MAX_BUDGET, day.items.length);
  return Math.max(1, Math.min(max, b === undefined ? defaultBudget(day.items.length) : b));
}
const currentBudget = computed(() => (active.value ? budgetFor(active.value) : 1));

function rank(m: MediaItem) {
  return (m.stars ?? 0) * 14 + (m.quality ?? 0) * 0.7 +
    (m.burstBest === 1 ? 9 : 0) + (m.timeConfidence ?? 0) * 3 + (m.kind === "video" ? 2 : 0);
}

function pick(day: Day, count: number): MediaItem[] {
  const ban = new Set(banned.value[day.key] ?? []);
  const pins = pinned.value[day.key] ?? [];
  const pool = day.items.filter((m) => !ban.has(m.id));
  if (!pool.length || count <= 0) return [];
  const byId = new Map(pool.map((m) => [m.id, m]));
  const ranked = [...pool].sort((a, b) => rank(b) - rank(a));
  const picked: MediaItem[] = [];
  for (const id of pins) {
    const m = byId.get(id);
    if (m && !picked.includes(m)) picked.push(m);
  }
  if (picked.length > count) picked.length = count;
  const usedBurst = new Set<number>();
  const usedDup = new Set<number>();
  for (const m of picked) {
    if (m.burstGroup !== null) usedBurst.add(m.burstGroup);
    if (m.dupGroup !== null) usedDup.add(m.dupGroup);
  }
  for (const m of ranked) {
    if (picked.length >= count) break;
    if (picked.includes(m)) continue;
    if (m.burstGroup !== null && usedBurst.has(m.burstGroup)) continue;
    if (m.dupGroup !== null && usedDup.has(m.dupGroup)) continue;
    picked.push(m);
    if (m.burstGroup !== null) usedBurst.add(m.burstGroup);
    if (m.dupGroup !== null) usedDup.add(m.dupGroup);
  }
  for (const m of ranked) {
    if (picked.length >= count) break;
    if (!picked.includes(m)) picked.push(m);
  }
  const hero = picked.slice().sort((a, b) => rank(b) - rank(a))[0];
  if (!hero) return [];
  const rest = picked.filter((m) => m !== hero).sort((a, b) => a.effTime - b.effTime);
  return [hero, ...rest];
}

const chosen = computed<MediaItem[]>(() => (active.value ? pick(active.value, currentBudget.value) : []));
const pickedIds = computed(() => new Set(chosen.value.map((m) => m.id)));
const bannedIds = computed(() => new Set(banned.value[active.value?.key ?? ""] ?? []));

const burstGroups = computed(() => {
  if (!active.value) return 0;
  const set = new Set<number>();
  for (const m of active.value.items) if (m.burstGroup !== null) set.add(m.burstGroup);
  return set.size;
});
const dateLine = computed(() => {
  const p = activeParts.value;
  return p ? p.m + " 月 " + p.d + " 日 · " + p.w : "";
});
const folio = computed(() => developed.value.length + 1);

// 把「为什么是这几张」摊开：创新交互不该是黑箱
const pickWhy = computed(() => {
  const day = active.value;
  if (!day || !chosen.value.length) return "";
  const best = chosen.value.filter((m) => m.burstBest === 1).length;
  const minQ = Math.min(...chosen.value.map((m) => m.quality ?? 0));
  const bits: string[] = [];
  if (best) bits.push("连拍最佳 " + best);
  bits.push("清晰度 " + Math.round(minQ) + "+");
  const left = day.items.length - chosen.value.length;
  if (left > 0) bits.push("舍 " + left + " 张");
  return "自动挑：" + bits.join(" · ");
});

function burstCount(m: MediaItem) {
  if (!active.value || m.burstGroup === null) return 0;
  let n = 0;
  for (const x of active.value.items) if (x.burstGroup === m.burstGroup) n++;
  return n;
}
function qClass(m: MediaItem) {
  const q = m.quality ?? 0;
  if (q >= 70) return "good";
  if (q >= 45) return "mid";
  return "low";
}
function frameTitle(m: MediaItem) {
  const bits = [m.filename];
  if (m.quality !== null) bits.push("质量 " + Math.round(m.quality));
  if (m.burstBest === 1) bits.push("连拍最佳");
  return bits.join(" · ");
}

function setBudget(n: number) {
  if (!active.value) return;
  const max = Math.min(MAX_BUDGET, active.value.items.length);
  const next = Math.max(1, Math.min(max, n));
  budget.value = { ...budget.value, [active.value.key]: next };
}

// 张数刻度可以「拖」：按住左右滑，页面实时重排
function scrubTo(clientX: number) {
  const el = ticksEl.value;
  if (!el || !active.value) return;
  const r = el.getBoundingClientRect();
  const max = Math.min(MAX_BUDGET, active.value.items.length);
  if (max <= 1) return;
  const t = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
  setBudget(1 + Math.round(t * (max - 1)));
}
let scrubbing = false;
function scrubStart(e: PointerEvent) {
  scrubbing = true;
  (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  scrubTo(e.clientX);
}
function scrubMove(e: PointerEvent) {
  if (scrubbing) scrubTo(e.clientX);
}
function scrubEnd() {
  scrubbing = false;
}

// 从胶片条把一张照片「拖」进这一页：直接钉选，页面立刻重排
function onDragStart(e: DragEvent, m: MediaItem) {
  dragId.value = m.id;
  if (e.dataTransfer) {
    e.dataTransfer.setData("text/plain", String(m.id));
    e.dataTransfer.effectAllowed = "copy";
  }
  dragging.value = true;
}
function onDragEnd() {
  dragging.value = false;
  dragOver.value = false;
  dragId.value = null;
}
function onDragOver(e: DragEvent) {
  if (!dragging.value) return;
  e.preventDefault();
  dragOver.value = true;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
}
function onDrop(e: DragEvent) {
  dragOver.value = false;
  // 有些 webview 在 drop 时拿不到 dataTransfer，用 dragstart 记下的 id 兜底
  const raw = e.dataTransfer?.getData("text/plain") || (dragId.value !== null ? String(dragId.value) : "");
  const day = active.value;
  if (!raw || !day) return;
  const id = Number(raw);
  const m = day.items.find((x) => x.id === id);
  if (!m) return;
  e.preventDefault();
  const key = day.key;
  const pin = [...new Set([...(pinned.value[key] ?? []), m.id])];
  pinned.value = { ...pinned.value, [key]: pin };
  banned.value = { ...banned.value, [key]: (banned.value[key] ?? []).filter((x) => x !== m.id) };
  const need = Math.min(MAX_BUDGET, day.items.length, pin.length);
  if (budgetFor(day) < need) budget.value = { ...budget.value, [key]: need };
  dragOver.value = false;
}

// 已显影的天 + 还没挑的天；同一天两边都出现时只算一次
const doneDays = computed(() => {
  const keys = new Set<string>();
  for (const d of days.value) keys.add(d.key);
  for (const p of developed.value) keys.add(p.key);
  return keys.size;
});
const donePct = computed(() => (doneDays.value ? Math.round((developed.value.length / doneDays.value) * 100) : 0));

function togglePic(m: MediaItem) {
  const day = active.value;
  if (!day || busy.value) return;
  const key = day.key;
  const pin = [...(pinned.value[key] ?? [])];
  const ban = [...(banned.value[key] ?? [])];
  if (pickedIds.value.has(m.id)) {
    pinned.value = { ...pinned.value, [key]: pin.filter((id) => id !== m.id) };
    banned.value = { ...banned.value, [key]: [...new Set([...ban, m.id])] };
  } else if (ban.includes(m.id)) {
    banned.value = { ...banned.value, [key]: ban.filter((id) => id !== m.id) };
  } else {
    const nextPin = [...new Set([...pin, m.id])];
    pinned.value = { ...pinned.value, [key]: nextPin };
    banned.value = { ...banned.value, [key]: ban.filter((id) => id !== m.id) };
    if (budgetFor(day) < nextPin.length) {
      budget.value = { ...budget.value, [key]: Math.min(MAX_BUDGET, nextPin.length) };
    }
  }
}

function resetDay() {
  if (!active.value) return;
  const key = active.value.key;
  pinned.value = { ...pinned.value, [key]: [] };
  banned.value = { ...banned.value, [key]: [] };
  budget.value = { ...budget.value, [key]: defaultBudget(active.value.items.length) };
}

function go(delta: number) {
  if (!days.value.length) return;
  const next = activeIdx.value + delta;
  if (next < 0) return;
  if (next > days.value.length - 1) {
    emit("toast", "已经是最后一天了");
    return;
  }
  activeIdx.value = next;
  nextTick(() => {
    if (stripEl.value) stripEl.value.scrollLeft = 0;
  });
}
function jump(key: string) {
  const i = days.value.findIndex((d) => d.key === key);
  if (i >= 0) activeIdx.value = i;
}
function skipDay() {
  if (activeIdx.value >= days.value.length - 1) emit("toast", "这是最后一天，显影后这一轮就收尾了");
  else activeIdx.value++;
}
const wait = (ms: number) => new Promise<void>((r) => window.setTimeout(r, ms));

async function load(silent = false) {
  if (!silent) {
    loading.value = true;
    error.value = null;
  }
  try {
    const [pending, keeps] = await Promise.all([
      api.listMedia({ decision: "unreviewed", sort: "time", dir: "desc", limit: 2000 }),
      api.listMedia({ decision: "keep", sort: "time", dir: "desc", limit: 2000 }),
    ]);
    items.value = pending.items;
    kept.value = keeps.items;
    stats.value = await api.stats();
    if (activeIdx.value > days.value.length - 1) {
      activeIdx.value = Math.max(0, days.value.length - 1);
    }
  } catch (e) {
    if (silent) emit("toast", "刷新失败：" + e);
    else error.value = String(e);
  } finally {
    if (!silent) loading.value = false;
  }
}

async function develop() {
  const day = active.value;
  if (!day || busy.value || !chosen.value.length) return;
  const keepIds = chosen.value.map((m) => m.id);
  const restIds = day.items.filter((m) => !keepIds.includes(m.id)).map((m) => m.id);
  busy.value = true;
  flashing.value = true;
  try {
    await Promise.all([api.developPage(keepIds, restIds), wait(320)]);
    rememberDay(day.key);
    // 页面「飞」进右侧精选册，再重读待挑 + 已保留（精选册由 kept 推导，撤销后自动回退）
    leaving.value = true;
    await wait(500);
    await load(true);
    const label = "显影 " + keepIds.length + " 张" + (restIds.length ? " · 其余 " + restIds.length + " 张转待定" : "");
    emit("changed", { label, steps: 1 });
  } catch (e) {
    emit("toast", "操作失败：" + e);
  } finally {
    busy.value = false;
    flashing.value = false;
    leaving.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (busy.value) return;
  const el = e.target as HTMLElement | null;
  if (el && /^(input|textarea|select)$/i.test(el.tagName)) return;
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const nativeButton = !!el && el.tagName === "BUTTON";
  const k = e.key;
  if (k === "ArrowUp" || k === "k" || k === "K") { e.preventDefault(); go(-1); }
  else if (k === "ArrowDown" || k === "j" || k === "J") { e.preventDefault(); go(1); }
  else if (k === "ArrowLeft") { e.preventDefault(); setBudget(currentBudget.value - 1); }
  else if (k === "ArrowRight") { e.preventDefault(); setBudget(currentBudget.value + 1); }
  else if (k === "Enter") { if (!nativeButton) { e.preventDefault(); develop(); } }
  else if (k === "x" || k === "X") { e.preventDefault(); skipDay(); }
  else if (k === "r" || k === "R") { e.preventDefault(); resetDay(); }
  else if (/^[1-6]$/.test(k) && active.value) { e.preventDefault(); setBudget(Number(k)); }
}

function onWheel(e: WheelEvent) {
  const el = stripEl.value;
  if (!el) return;
  el.scrollLeft += e.deltaY !== 0 ? e.deltaY : e.deltaX;
}

watch(() => props.revision, () => { load(true); });
onMounted(async () => {
  await load();
  root.value?.focus({ preventScroll: true });
});
</script>

<template>
  <section class="story" ref="root" tabindex="0" @keydown="onKey" aria-label="故事线挑图">
    <div v-if="loading" class="story-loading">
      <span class="loader"></span>
      <p>正在翻开相册…</p>
    </div>

    <div v-else-if="error" class="story-loading">
      <p>读不到待挑的照片：{{ error }}</p>
      <button class="primary" @click="load">重试</button>
    </div>

    <div v-else-if="!days.length" class="story-done">
      <div class="done-mark">✓</div>
      <h2>这一轮，都看过了</h2>
      <p>{{ stats?.keeps ?? 0 }} 张留了下来。可以去网格里补挑，或者直接成册。</p>
      <div class="done-actions">
        <button class="primary" @click="emit('grid')">回到网格</button>
        <button @click="emit('export')">导出精选集</button>
      </div>
      <div class="done-stats">
        <span>保留 <b>{{ stats?.keeps ?? 0 }}</b></span>
        <span>待定 <b>{{ stats?.pendings ?? 0 }}</b></span>
        <span>淘汰 <b>{{ stats?.rejects ?? 0 }}</b></span>
      </div>
    </div>

    <template v-else>
      <aside class="timeline">
        <div class="tl-head">
          <span class="tl-kicker">待挑</span>
          <span class="tl-count">{{ days.length }} 天</span>
        </div>
        <div class="tl-scroll">
          <template v-for="mo in months" :key="mo.label">
            <div class="tl-month">{{ mo.label }}</div>
            <button
              v-for="d in mo.days"
              :key="d.key"
              class="tl-day"
              :class="{ on: active && active.key === d.key }"
              @click="jump(d.key)"
            >
              <span class="tl-dot" :style="{ '--w': Math.min(1, d.items.length / 34) }"></span>
              <span class="tl-date">{{ parts(d.ts).m }}/{{ pad2(parts(d.ts).d) }}</span>
              <span class="tl-week">{{ parts(d.ts).w }}</span>
              <span class="tl-n">{{ d.items.length }}</span>
            </button>
          </template>
        </div>
        <div class="tl-progress">
          <i :style="{ width: donePct + '%' }"></i>
        </div>
        <div class="tl-foot">
          <span>已显影</span><b>{{ developed.length }}</b><span>/ {{ doneDays }} 天</span>
        </div>
      </aside>

      <main class="stage">
        <div v-if="active && activeParts" class="stage-inner" :key="active.key">
          <header class="day-head">
            <div class="day-title">
              <span class="day-num">{{ pad2(activeParts.d) }}</span>
              <span class="day-unit">{{ activeParts.m }} 月 · {{ activeParts.w }}</span>
              <span class="day-year">{{ activeParts.y }}</span>
            </div>
            <div class="day-meta">
              <span>{{ active.items.length }} 张待挑</span>
              <span class="mdot"></span>
              <span v-if="burstGroups">{{ burstGroups }} 组连拍</span>
              <span v-if="burstGroups" class="mdot"></span>
              <span>第 {{ folio }} 页</span>
            </div>
          </header>

          <div class="page-area">
            <div class="spread-wrap">
              <div class="paper-under" aria-hidden="true"></div>
              <div
                class="spread"
                :data-n="chosen.length"
                :class="{ developing: flashing, leaving, dragover: dragOver }"
                @dragover="onDragOver"
                @dragleave="dragOver = false"
                @drop="onDrop"
              >
              <TransitionGroup tag="div" name="slot" class="slots">
                <figure
                  v-for="(m, i) in chosen"
                  :key="m.id"
                  class="slot"
                  :style="{ '--i': i }"
                  :title="frameTitle(m) + ' · 点「换掉」从候选里补一张'"
                >
                  <img :src="thumbUrl(m)" :alt="m.filename" draggable="false" />
                  <figcaption v-if="m.burstBest === 1" class="slot-tag">连拍最佳</figcaption>
                  <button class="slot-swap" @click.stop="togglePic(m)">换掉</button>
                </figure>
              </TransitionGroup>
              <div v-if="!chosen.length" class="slot-empty">这一天没有可挑的照片</div>
              <div class="paper-foot">
                <span>{{ dateLine }}</span>
                <span class="folio">第 {{ folio }} 页 · {{ chosen.length }} 张</span>
              </div>
              <div v-if="dragOver" class="drop-hint">松手，加入这一页</div>
              <div v-if="flashing" class="shutter"></div>
              </div>
            </div>

            <div class="compose">
              <div class="compose-count">
                <span class="cc-num">{{ currentBudget }}</span>
                <div class="cc-text">
                  <span class="cc-k">这一页，留几张？</span>
                  <span class="cc-sub">{{ pickWhy || '按住刻度左右滑，页面会实时重排' }}</span>
                </div>
              </div>
              <div
                class="ticks"
                ref="ticksEl"
                role="group"
                aria-label="保留张数，可左右拖动"
                @pointerdown.prevent="scrubStart"
                @pointermove="scrubMove"
                @pointerup="scrubEnd"
                @pointercancel="scrubEnd"
              >
                <span class="ticks-line" aria-hidden="true"></span>
                <button
                  v-for="n in Math.min(MAX_BUDGET, active.items.length)"
                  :key="n"
                  :class="{ lit: n <= currentBudget, at: n === currentBudget }"
                  :aria-pressed="n === currentBudget"
                  :title="'留 ' + n + ' 张'"
                  @click="setBudget(n)"
                >{{ n }}<i></i></button>
              </div>
              <div class="compose-actions">
                <button class="develop" :disabled="busy || !chosen.length" @click="develop">
                  <span>显影</span><kbd>↵</kbd>
                </button>
                <button class="skip" :disabled="busy" @click="skipDay">跳过 <kbd>X</kbd></button>
              </div>
            </div>
          </div>

          <div class="strip-wrap">
            <div class="strip-head">
              <span>这一天 · {{ active.items.length }} 张 <b class="strip-picked">已选 {{ chosen.length }}</b></span>
              <span class="strip-hint"><kbd>←</kbd><kbd>→</kbd> 张数 · 点照片加入 · 拖进页面 · <kbd>R</kbd> 重置</span>
            </div>
            <div class="strip" ref="stripEl" @wheel.prevent="onWheel">
              <button
                v-for="m in active.items"
                :key="m.id"
                class="frame"
                :class="{ picked: pickedIds.has(m.id), banned: bannedIds.has(m.id) }"
                :title="frameTitle(m) + (pickedIds.has(m.id) ? ' · 已选（点一下移出）' : ' · 点一下加入，或直接拖进页面')"
                draggable="true"
                @click="togglePic(m)"
                @dragstart="onDragStart($event, m)"
                @dragend="onDragEnd"
              >
                <img :src="thumbUrl(m)" alt="" loading="lazy" draggable="false" />
                <span v-if="m.burstBest === 1" class="f-badge best">最佳</span>
                <span v-else-if="burstCount(m) > 1" class="f-badge">×{{ burstCount(m) }}</span>
                <span v-if="m.stars" class="f-star">★{{ m.stars }}</span>
                <span class="f-q" :class="qClass(m)"></span>
              </button>
            </div>
          </div>
        </div>
      </main>

      <aside class="album">
        <div class="album-head">
          <span class="album-kicker">精选册</span>
          <span class="album-count">{{ stats?.keeps ?? 0 }} 张</span>
        </div>
        <div class="album-scroll">
          <div v-if="!developed.length" class="album-empty">
            <div class="ghost-page" aria-hidden="true">
              <span></span><span></span><span></span><span></span>
            </div>
            <p>按 <b>显影</b>，这一页<br />就收进册子里</p>
            <span class="ghost-hint">第 1 页 · 会显示缩略样张</span>
          </div>
          <div
            v-for="(p, i) in developed"
            :key="p.key"
            class="mini"
            :style="{ '--i': i, '--rot': i % 2 ? '0.7deg' : '-0.5deg' }"
          >
            <div class="mini-grid" :data-n="p.thumbs.length">
              <img v-for="(t, j) in p.thumbs" :key="j" :src="t" alt="" draggable="false" />
            </div>
            <div class="mini-foot">
              <span>{{ parts(p.ts).m }} 月 {{ parts(p.ts).d }} 日</span>
              <b>{{ p.count }} 张</b>
            </div>
          </div>
        </div>
        <button class="album-export" @click="emit('export')">
          <Icon name="export" :size="14" /> 导出精选集
        </button>
      </aside>
    </template>
  </section>
</template>

<style scoped>
.story {
  flex: 1;
  min-height: 0;
  display: flex;
  background:
    radial-gradient(54% 48% at 50% 6%, oklch(from var(--accent) l c h / .06), transparent 70%);
  outline: none;
}
.story-loading {
  flex: 1;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 14px;
  color: var(--text-3);
}
.loader {
  width: 26px; height: 26px;
  border-radius: 50%;
  border: 2px solid var(--border-2);
  border-top-color: var(--accent);
  animation: spin .9s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

/* ── 左：时间轴 ── */
.timeline {
  width: 178px;
  flex: none;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--border-1);
  background: color-mix(in oklab, var(--surface-1) 36%, transparent);
}
.tl-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 18px 16px 10px;
}
.tl-kicker {
  font-size: var(--fs-1);
  letter-spacing: .16em;
  text-transform: uppercase;
  color: var(--text-4);
  font-weight: 600;
}
.tl-count { font-size: var(--fs-2); color: var(--text-2); font-variant-numeric: tabular-nums; }
.tl-scroll { flex: 1; overflow-y: auto; padding: 0 8px 12px; }
.tl-month {
  position: sticky;
  top: 0;
  z-index: 1;
  padding: 10px 8px 5px;
  font-size: var(--fs-1);
  color: var(--text-4);
  background: linear-gradient(var(--bg-1) 70%, transparent);
  letter-spacing: .04em;
}
.tl-day {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text-2);
  font-size: var(--fs-2);
  transition: background var(--dur-2) var(--ease-out), color var(--dur-2) var(--ease-out);
}
.tl-day:hover { background: var(--hover); color: var(--text-1); }
.tl-day.on {
  background: var(--accent-soft);
  color: var(--text-1);
  box-shadow: inset 2px 0 0 var(--accent);
}
.tl-dot {
  width: calc(5px + var(--w, 0) * 7px);
  height: calc(5px + var(--w, 0) * 7px);
  border-radius: 50%;
  background: var(--text-4);
  flex: none;
  transition: background var(--dur-2) var(--ease-out);
}
.tl-day.on .tl-dot { background: var(--accent); }
.tl-date { font-variant-numeric: tabular-nums; font-family: var(--font-mono); font-size: var(--fs-1); }
.tl-week { color: var(--text-4); font-size: var(--fs-1); }
.tl-n {
  margin-left: auto;
  font-family: var(--font-mono);
  font-size: var(--fs-1);
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.tl-day.on .tl-n { color: var(--accent-ink); }
.tl-progress {
  flex: none;
  height: 2px;
  margin: 0 16px;
  border-radius: 2px;
  background: var(--border-1);
  overflow: hidden;
}
.tl-progress i {
  display: block;
  height: 100%;
  background: var(--accent);
  transition: width var(--dur-4) var(--ease-out);
}
.tl-foot {
  flex: none;
  display: flex;
  align-items: baseline;
  gap: 4px;
  padding: 10px 16px 14px;
  border-top: 1px solid var(--border-1);
  font-size: var(--fs-1);
  color: var(--text-4);
}
.tl-foot b { color: var(--text-2); font-family: var(--font-mono); font-size: var(--fs-3); }

/* ── 中：今日成页 ── */
.stage {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 16px 24px 14px;
  display: flex;
  flex-direction: column;
}
.stage-inner {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: 100%;
  min-height: 0;
  animation: dayIn var(--dur-4) var(--ease-spring) both;
}
.page-area {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 14px;
}
@keyframes dayIn {
  from { opacity: 0; transform: translateY(14px); }
  to { opacity: 1; transform: none; }
}
.day-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
}
.day-title { display: flex; align-items: baseline; gap: 10px; }
.day-num {
  font-family: ui-serif, "Songti SC", "Noto Serif SC", Georgia, serif;
  font-size: 46px;
  line-height: .9;
  letter-spacing: -.02em;
  font-variant-numeric: tabular-nums;
  color: var(--text-1);
}
.day-unit { font-size: var(--fs-4); color: var(--text-2); font-weight: 500; }
.day-year { font-family: var(--font-mono); font-size: var(--fs-1); color: var(--text-4); }
.day-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 4px;
  font-size: var(--fs-1);
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.mdot { width: 3px; height: 3px; border-radius: 50%; background: var(--text-4); }

.spread-wrap {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 0;
  animation: pageTurn 440ms var(--ease-out) both;
  transform-origin: 18% 62%;
}
/* 纸下面再垫一页：像真正的相册，而不是一张截图 */
.paper-under {
  grid-area: 1 / 1;
  width: min(100%, calc((100vh - 332px) * 1.3333), 900px);
  min-width: 320px;
  aspect-ratio: 4 / 3;
  border-radius: 3px;
  background: oklch(92.5% .012 95);
  transform: rotate(-.9deg) translate(9px, 7px);
  box-shadow: 0 18px 40px -34px oklch(0% 0 0 / .5);
  pointer-events: none;
  z-index: 0;
}
@keyframes pageTurn {
  from { opacity: 0; transform: perspective(1500px) rotateY(-7deg) translateX(20px) scale(.99); }
  to { opacity: 1; transform: none; }
}
.spread {
  position: relative;
  grid-area: 1 / 1;
  z-index: 1;
  width: min(100%, calc((100vh - 332px) * 1.3333), 900px);
  min-width: 320px;
  aspect-ratio: 4 / 3;
  padding: 14px 14px 8px;
  border-radius: 3px;
  background: var(--paper, oklch(97% .012 95));
  color: oklch(30% .012 85);
  box-shadow:
    0 30px 60px -30px oklch(10% .02 150 / .55),
    0 2px 6px oklch(10% .02 150 / .18),
    0 0 0 .5px oklch(30% .02 95 / .18);
  display: flex;
  flex-direction: column;
  transition: transform var(--dur-4) var(--ease-spring), box-shadow var(--dur-4) var(--ease-out);
}
.spread.developing { transform: scale(.985) translateY(2px); }
.spread.dragover {
  outline: 2px dashed var(--accent);
  outline-offset: 5px;
}
.drop-hint {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  padding: 8px 16px;
  border-radius: var(--r-pill);
  background: var(--glass-strong);
  border: 1px solid var(--accent);
  color: var(--accent-ink, var(--accent));
  font-size: var(--fs-2);
  font-weight: 600;
  backdrop-filter: blur(var(--blur-2));
  pointer-events: none;
  animation: toastIn var(--dur-2) var(--ease-spring);
}
.spread.leaving {
  pointer-events: none;
  opacity: 0;
  transform: translateX(34vw) translateY(-3vh) scale(.14) rotate(3deg);
  transition:
    transform 460ms cubic-bezier(.4, 0, .2, 1),
    opacity 460ms cubic-bezier(.55, 0, .8, .2);
}
.slots {
  flex: 1;
  min-height: 0;
  display: grid;
  gap: 6px;
}
.spread[data-n="1"] .slots { grid-template-columns: 1fr; }
.spread[data-n="2"] .slots { grid-template-columns: 1fr 1fr; }
.spread[data-n="3"] .slots { grid-template-columns: 1.5fr 1fr; grid-template-rows: 1fr 1fr; }
.spread[data-n="3"] .slot:first-child { grid-row: span 2; }
.spread[data-n="4"] .slots { grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; }
.spread[data-n="5"] .slots { grid-template-columns: repeat(3, 1fr); grid-template-rows: 1fr 1fr; }
.spread[data-n="5"] .slot:first-child { grid-column: span 2; }
.spread[data-n="6"] .slots { grid-template-columns: repeat(3, 1fr); grid-template-rows: 1fr 1fr; }
.slot {
  position: relative;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  border-radius: 1px;
  background: oklch(88% .008 95);
}
.slot img { width: 100%; height: 100%; object-fit: cover; display: block; transition: transform var(--dur-4) var(--ease-out), filter var(--dur-2) var(--ease-out); }
.slot:hover img { transform: scale(1.02); filter: brightness(.9); }
.slot-tag {
  position: absolute;
  left: 7px;
  top: 7px;
  padding: 2px 6px;
  border-radius: 3px;
  background: oklch(0% 0 0 / .5);
  color: oklch(98% 0 0);
  font-size: 10px;
  font-family: var(--font-mono);
  letter-spacing: .02em;
  backdrop-filter: blur(6px);
  pointer-events: none;
}
.slot-swap {
  position: absolute;
  right: 7px;
  bottom: 7px;
  height: 24px;
  padding: 0 9px;
  border: none;
  border-radius: var(--r-pill);
  background: oklch(0% 0 0 / .58);
  color: oklch(98% 0 0);
  font-size: 11px;
  font-weight: 600;
  opacity: 0;
  transform: translateY(5px);
  transition: opacity var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-spring), background var(--dur-2) var(--ease-out);
  backdrop-filter: blur(6px);
}
.slot:hover .slot-swap, .slot-swap:focus-visible { opacity: 1; transform: translateY(0); }
.slot-swap:hover { background: var(--danger); }
.slot-empty {
  flex: 1;
  display: grid;
  place-items: center;
  border: 1px dashed oklch(70% .01 95);
  color: oklch(52% .01 85);
  font-size: var(--fs-2);
}
.paper-foot {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 2px 0;
  font-family: ui-serif, "Songti SC", "Noto Serif SC", Georgia, serif;
  font-size: 11px;
  color: oklch(45% .012 85);
  letter-spacing: .02em;
}
.folio { font-family: var(--font-mono); font-size: 10px; letter-spacing: .04em; color: oklch(58% .01 85); }

.slot-enter-active { transition: opacity var(--dur-3) var(--ease-out), transform var(--dur-4) var(--ease-spring); transition-delay: calc(var(--i, 0) * 30ms); }
.slot-leave-active { transition: opacity var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-out); position: absolute; }
.slot-enter-from { opacity: 0; transform: scale(.94) translateY(10px); }
.slot-leave-to { opacity: 0; transform: scale(.96); }
.slot-move { transition: transform var(--dur-4) var(--ease-spring); }

.shutter {
  position: absolute;
  inset: 0;
  pointer-events: none;
  border-radius: 3px;
  background: radial-gradient(80% 70% at 50% 50%, oklch(100% 0 0 / .92), oklch(100% 0 0 / 0) 72%);
  animation: shutter var(--dur-4) var(--ease-out) forwards;
}
@keyframes shutter {
  0% { opacity: 0; transform: scale(1.02); }
  18% { opacity: 1; }
  100% { opacity: 0; transform: scale(1.04); }
}

.compose {
  display: flex;
  align-items: center;
  gap: 18px;
}
.compose-count { display: flex; align-items: center; gap: 10px; min-width: 210px; }
.cc-num {
  font-family: var(--font-mono);
  font-size: 30px;
  font-weight: 600;
  line-height: 1;
  color: var(--text-1);
  font-variant-numeric: tabular-nums;
}
.cc-text { display: flex; flex-direction: column; gap: 2px; }
.cc-k { font-size: var(--fs-3); font-weight: 600; color: var(--text-1); }
.cc-sub { font-size: var(--fs-1); color: var(--text-4); }
/* 刻度尺：可点、可拖，选中的数字下方有一枚「曝光标记」 */
.ticks {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: 0;
  padding: 2px 0 0;
  touch-action: none;
  cursor: ew-resize;
  user-select: none;
}
.ticks-line {
  position: absolute;
  left: 14px;
  right: 14px;
  top: 27px;
  height: 1px;
  background: var(--border-2);
}
.ticks button {
  position: relative;
  width: 34px;
  height: 38px;
  padding: 3px 0 0;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text-4);
  font-family: var(--font-mono);
  font-size: var(--fs-1);
  font-variant-numeric: tabular-nums;
  transition: color var(--dur-2) var(--ease-out), transform var(--dur-1) var(--ease-out);
}
.ticks button i {
  position: absolute;
  left: 50%;
  bottom: 3px;
  width: 5px;
  height: 5px;
  margin-left: -2.5px;
  border-radius: 50%;
  background: var(--border-2);
  transition: background var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-spring), box-shadow var(--dur-2) var(--ease-out);
}
.ticks button:hover { color: var(--text-1); }
.ticks button:hover i { background: var(--text-3); }
.ticks button.lit { color: var(--text-1); }
.ticks button.lit i { background: var(--accent-ink, var(--accent)); }
.ticks button.at { color: var(--text-1); font-weight: 700; transform: translateY(-2px); }
.ticks button.at i {
  background: var(--accent);
  transform: scale(1.7);
  box-shadow: 0 0 0 4px var(--accent-soft);
}
.ticks button:active { transform: translateY(0) scale(.94); }
.ticks button:disabled { opacity: .25; }
.compose-actions { display: flex; align-items: center; gap: 8px; margin-left: auto; }
.develop {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  height: 42px;
  padding: 0 18px;
  border: none;
  border-radius: var(--r-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: var(--fs-3);
  font-weight: 650;
  letter-spacing: .02em;
  box-shadow: 0 10px 24px -12px oklch(from var(--accent) l c h / .8);
  transition: transform var(--dur-1) var(--ease-out), background var(--dur-2) var(--ease-out), box-shadow var(--dur-2) var(--ease-out);
}
.develop:hover { background: var(--accent-strong); transform: translateY(-1px); box-shadow: 0 14px 30px -12px oklch(from var(--accent) l c h / .9); }
.develop:active { transform: translateY(0) scale(.98); }
.develop kbd {
  display: inline-grid;
  place-items: center;
  min-width: 18px;
  height: 18px;
  padding: 0 4px;
  border-radius: 4px;
  background: oklch(from var(--on-accent) l c h / .16);
  color: inherit;
  font-size: 10px;
  line-height: 1;
}
.skip { height: 34px; color: var(--text-3); }
.skip kbd { margin-left: 2px; }

.strip-wrap { flex: none; }
.strip-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 2px 6px;
  font-size: var(--fs-1);
  color: var(--text-3);
}
.strip-hint { display: inline-flex; align-items: center; gap: 4px; color: var(--text-4); }
.strip {
  display: flex;
  gap: 7px;
  overflow-x: auto;
  overflow-y: hidden;
  padding: 8px 2px 10px;
  scrollbar-width: thin;
  scroll-behavior: smooth;
}
.strip-picked {
  margin-left: 6px;
  padding: 1px 6px;
  border-radius: var(--r-pill);
  background: var(--accent-soft);
  color: var(--accent-ink, var(--accent));
  font-size: 10px;
  font-weight: 600;
}
.frame {
  position: relative;
  flex: none;
  width: 94px;
  height: 68px;
  padding: 0;
  border: none;
  border-radius: 7px;
  overflow: hidden;
  background: var(--thumb-bg);
  opacity: .62;
  transform: translateY(0);
  transition: opacity var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-spring), box-shadow var(--dur-2) var(--ease-out);
}
.frame img { width: 100%; height: 100%; object-fit: cover; display: block; }
.frame:hover { opacity: 1; transform: translateY(-2px); box-shadow: var(--shadow-sm); }
.frame:active { cursor: grabbing; }
.frame.picked {
  opacity: 1;
  transform: translateY(-4px);
  box-shadow: 0 0 0 2px var(--accent), 0 12px 22px -14px oklch(0% 0 0 / .6);
}
.frame.banned { opacity: .26; }
.frame.banned::after {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(135deg, transparent 47%, var(--danger) 47%, var(--danger) 53%, transparent 53%);
  opacity: .8;
}
.f-badge {
  position: absolute;
  left: 4px;
  top: 4px;
  padding: 0 4px;
  height: 14px;
  display: inline-flex;
  align-items: center;
  border-radius: 3px;
  background: oklch(0% 0 0 / .56);
  color: oklch(98% 0 0);
  font-size: 9px;
  font-family: var(--font-mono);
  backdrop-filter: blur(4px);
}
.f-badge.best { background: var(--accent); color: var(--on-accent); font-weight: 700; }
.f-star {
  position: absolute;
  right: 4px;
  top: 4px;
  font-size: 9px;
  color: oklch(98% 0 0);
  text-shadow: 0 1px 3px oklch(0% 0 0 / .8);
  font-weight: 600;
}
.f-q {
  position: absolute;
  right: 5px;
  bottom: 5px;
  width: 5px;
  height: 5px;
  border-radius: 50%;
  box-shadow: 0 0 0 1.5px oklch(0% 0 0 / .3);
}
.f-q.good { background: var(--ok); }
.f-q.mid { background: var(--warn); }
.f-q.low { background: var(--danger); }

/* ── 右：精选册 ── */
.album {
  width: 236px;
  flex: none;
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--border-1);
  background: color-mix(in oklab, var(--surface-1) 36%, transparent);
}
.album-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 18px 16px 10px;
}
.album-kicker {
  font-size: var(--fs-1);
  letter-spacing: .16em;
  text-transform: uppercase;
  color: var(--text-4);
  font-weight: 600;
}
.album-count { font-size: var(--fs-2); color: var(--text-2); font-variant-numeric: tabular-nums; }
.album-scroll { flex: 1; overflow-y: auto; padding: 2px 16px 12px; display: flex; flex-direction: column; gap: 12px; }
.album-empty { display: grid; justify-items: center; gap: 12px; padding: 18px 0; color: var(--text-4); font-size: var(--fs-1); text-align: center; }
.album-empty b { color: var(--accent-ink, var(--accent)); font-weight: 600; }
.ghost-page {
  width: 104px;
  aspect-ratio: 4 / 3;
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
  gap: 2px;
  padding: 6px;
  border-radius: 2px;
  background: oklch(97% .012 95);
  box-shadow: 0 10px 20px -14px oklch(0% 0 0 / .7), 0 0 0 .5px oklch(30% .02 95 / .14);
  transform: rotate(-2.5deg);
}
.ghost-page span { background: oklch(90% .01 95); border-radius: 1px; }
.ghost-page span:first-child { background: oklch(86% .02 160); }
.ghost-hint { font-family: var(--font-mono); font-size: 9px; color: var(--text-4); }
.mini {
  background: oklch(97% .012 95);
  padding: 5px 5px 4px;
  border-radius: 2px;
  color: oklch(30% .012 85);
  box-shadow: 0 8px 18px -12px oklch(0% 0 0 / .55), 0 0 0 .5px oklch(30% .02 95 / .16);
  transform: rotate(var(--rot, 0deg));
  animation: pageIn var(--dur-4) var(--ease-spring) both;
}
@keyframes pageIn {
  from { opacity: 0; transform: translateY(-14px) rotate(2deg) scale(.96); }
  to { opacity: 1; transform: rotate(var(--rot, 0deg)); }
}
.mini-grid { display: grid; gap: 2px; aspect-ratio: 4 / 3; }
.mini-grid[data-n="1"] { grid-template-columns: 1fr; }
.mini-grid[data-n="2"] { grid-template-columns: 1fr 1fr; }
.mini-grid[data-n="3"] { grid-template-columns: 1.4fr 1fr; grid-template-rows: 1fr 1fr; }
.mini-grid[data-n="3"] img:first-child { grid-row: span 2; }
.mini-grid[data-n="4"] { grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; }
.mini-grid img { width: 100%; height: 100%; object-fit: cover; display: block; }
.mini-foot {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 5px 1px 1px;
  font-family: ui-serif, "Songti SC", Georgia, serif;
  font-size: 10px;
  color: oklch(45% .012 85);
}
.mini-foot b { font-family: var(--font-mono); font-size: 9px; color: oklch(55% .01 85); }
.album-export {
  flex: none;
  margin: 0 12px 12px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid var(--border-1);
  border-radius: var(--r-pill);
  background: transparent;
  color: var(--text-2);
  font-size: var(--fs-2);
}
.album-export:hover { border-color: var(--border-2); color: var(--text-1); background: var(--hover); }

/* ── 收尾 ── */
.story-done {
  flex: 1;
  display: grid;
  align-content: center;
  justify-items: center;
  gap: 10px;
  text-align: center;
  animation: dayIn var(--dur-5) var(--ease-spring) both;
}
.done-mark {
  width: 54px;
  height: 54px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent-ink);
  font-size: 26px;
  margin-bottom: 6px;
}
.story-done h2 { font-size: var(--fs-5); font-weight: 600; letter-spacing: -.01em; }
.story-done p { color: var(--text-3); font-size: var(--fs-3); }
.done-actions { display: flex; gap: 8px; margin-top: 8px; }
.done-stats { display: flex; gap: 18px; margin-top: 14px; color: var(--text-4); font-size: var(--fs-2); }
.done-stats b { color: var(--text-1); font-family: var(--font-mono); }

@media (prefers-reduced-motion: reduce) {
  .spread-wrap { animation: none; }
  .slots, .slot { transition: none; }
}

@media (max-width: 1240px) {
  .album { width: 204px; }
}
@media (max-width: 1120px) {
  .album { display: none; }
}
@media (max-width: 920px) {
  .timeline { display: none; }
  .stage { padding: 14px 16px; }
  .spread, .paper-under { width: 100%; min-width: 0; }
  .compose-count { min-width: 0; }
  .cc-sub { display: none; }
  .strip-hint { display: none; }
}
</style>
