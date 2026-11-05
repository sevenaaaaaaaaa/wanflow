<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
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
  /** 这一页最后一张的时间；按天成页时是当天最后一张，按事件成页时是这一段最后一张 */
  endTs: number;
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

// 首次运行的上手引导：不是说明书，是跟着做一遍
const TOUR_KEY = "wanflow-storyline-tour";
interface TourStep { title: string; body: string; sel: string }
const TOUR_STEPS: TourStep[] = [
  { title: "左边是日子", body: "每一天一页。点一天就翻到那天——你只需要按天看，不用一张张判。", sel: ".timeline" },
  { title: "中间是这一页", body: "系统已经先替你挑了一版。点页里的照片可以换掉，把下面的胶片拖进来可以钉住。", sel: ".spread" },
  { title: "决定留几张", body: "按 1–6 或点刻度，这一页会跟着变成 1–6 张的版式。改一下试试。", sel: ".compose" },
  { title: "按「显影」收工", body: "留下的进精选册，其余转待定；不满意随时 ⌘Z 整组撤销。", sel: ".develop" },
];
const tourOpen = ref(false);
const tourStep = ref(0);
const tourPending = ref(false);
const tourRect = ref<{ left: number; top: number; width: number; height: number } | null>(null);
let tourTimer = 0;

const budget = ref<Record<string, number>>({});
const pinned = ref<Record<string, number[]>>({});
const banned = ref<Record<string, number[]>>({});

interface Page {
  key: string;
  ts: number;
  endTs: number;
  count: number;
  thumbs: string[];
}
const kept = ref<MediaItem[]>([]);
// 精选册记「显影时的那一页」，而不是记日期：keys 只是索引，
// 日期与跨度随记录走，按天 / 按事件两种成页方式才能并存同一本册子。
const PAGES_KEY = "wanflow-developed-pages";
const LEGACY_DAYS_KEY = "wanflow-developed-days";
interface DevelopedPage { ids: number[]; ts: number; endTs: number; mode: "day" | "event" }
const developedPages = ref<Record<string, DevelopedPage>>({});
function keyToTs(key: string) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(key);
  if (!m) return 0;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])).getTime() / 1000;
}
try {
  const raw = localStorage.getItem(PAGES_KEY);
  if (raw) {
    developedPages.value = JSON.parse(raw) as Record<string, DevelopedPage>;
  } else {
    // 旧格式：Record<dayKey, keptIds> 或 string[]；迁移成带日期跨度的记录
    const legacy = localStorage.getItem(LEGACY_DAYS_KEY);
    if (legacy) {
      const parsed = JSON.parse(legacy);
      const out: Record<string, DevelopedPage> = {};
      if (Array.isArray(parsed)) {
        for (const key of parsed) {
          const ts = keyToTs(String(key));
          out[String(key)] = { ids: [], ts, endTs: ts, mode: "day" };
        }
      } else if (parsed && typeof parsed === "object") {
        for (const [key, ids] of Object.entries(parsed)) {
          const ts = keyToTs(key);
          out[key] = { ids: (ids as number[]) ?? [], ts, endTs: ts, mode: "day" };
        }
      }
      developedPages.value = out;
    }
  }
} catch { /* 本地存储不可用就不记忆 */ }
function rememberPage(page: Day, ids: number[]) {
  const rec: DevelopedPage = { ids, ts: page.ts, endTs: page.endTs, mode: groupMode.value };
  // 同一批照片只该在册子里出现一次：新页完整覆盖了旧页时，把旧页换掉
  const sig = new Set(ids);
  const next: Record<string, DevelopedPage> = {};
  for (const [key, prev] of Object.entries(developedPages.value)) {
    if (key === page.key) continue;
    if (ids.length && prev.ids.length && prev.ids.every((id) => sig.has(id))) continue;
    next[key] = prev;
  }
  next[page.key] = rec;
  developedPages.value = next;
  try { localStorage.setItem(PAGES_KEY, JSON.stringify(next)); } catch { /* 忽略 */ }
}
const developed = computed<Page[]>(() => {
  const keptById = new Map(kept.value.map((m) => [m.id, m]));
  const out: Page[] = [];
  for (const [key, rec] of Object.entries(developedPages.value)) {
    let list: MediaItem[];
    if (rec.ids.length) {
      // 当初显影的那几张现在必须仍是 keep；撤销后自然隐藏，重做后自动回来
      list = rec.ids.map((id) => keptById.get(id)).filter((m): m is MediaItem => !!m);
      if (list.length !== rec.ids.length) continue;
    } else {
      // 更早的记录没存 ids：退回按天取这本册子里的 keep
      list = kept.value.filter((m) => dayKey(m.effTime) === key);
      if (!list.length) continue;
    }
    list.sort((a, b) => a.effTime - b.effTime);
    out.push({
      key,
      ts: rec.ts || list[0].effTime,
      endTs: rec.endTs || list[list.length - 1].effTime,
      count: list.length,
      thumbs: list.slice(0, 4).map((m) => thumbUrl(m)),
    });
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

// 成页方式：day = 日历天；event = 一次连续拍摄（间隔 > 6 小时就断，跨午夜接着拍算同一段）
// 设备 / 文件名不参与断开：家人换手机拍同一场景时，它们会把同一段拆散。
const GROUP_MODE_KEY = "wanflow-group-mode";
const groupMode = ref<"day" | "event">("day");
try { if (localStorage.getItem(GROUP_MODE_KEY) === "event") groupMode.value = "event"; } catch { /* 忽略 */ }
const EVENT_GAP_MAX = 6 * 3600; // 只按拍摄间隔兜底
// 连拍组永远不拆（同一组照片的时间跨度不构成新事件）。
// 是否同一场景交给内容（scene）判断，这里不做设备猜测。
function isSessionBreak(prev: MediaItem, next: MediaItem) {
  if (prev.burstGroup !== null && prev.burstGroup === next.burstGroup) return false;
  return next.effTime - prev.effTime > EVENT_GAP_MAX;
}

function groupByDay(list: MediaItem[]): Day[] {
  const map = new Map<string, MediaItem[]>();
  for (const m of list) {
    const key = dayKey(m.effTime);
    const group = map.get(key);
    if (group) group.push(m);
    else map.set(key, [m]);
  }
  const out: Day[] = [];
  for (const [key, group] of map) {
    group.sort((a, b) => a.effTime - b.effTime);
    out.push({ key, ts: group[0].effTime, endTs: group[group.length - 1].effTime, items: group });
  }
  out.sort((a, b) => b.ts - a.ts);
  return out;
}
function groupByEvent(list: MediaItem[]): Day[] {
  const sorted = [...list].sort((a, b) => a.effTime - b.effTime);
  const out: Day[] = [];
  let run: MediaItem[] = [];
  const flush = () => {
    if (!run.length) return;
    out.push({ key: "evt-" + run[0].id, ts: run[0].effTime, endTs: run[run.length - 1].effTime, items: run });
    run = [];
  };
  for (const m of sorted) {
    if (run.length && isSessionBreak(run[run.length - 1], m)) flush();
    run.push(m);
  }
  flush();
  out.sort((a, b) => b.ts - a.ts);
  return out;
}
const days = computed<Day[]>(() => (groupMode.value === "day" ? groupByDay(items.value) : groupByEvent(items.value)));

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

const isEvent = computed(() => groupMode.value === "event");
const groupUnit = computed(() => (isEvent.value ? "个事件" : "天"));
const groupShort = computed(() => (isEvent.value ? "段" : "天"));
const groupNoun = computed(() => (isEvent.value ? "这一段" : "这一天"));

function timeOf(ts: number) {
  const d = new Date(ts * 1000);
  return pad2(d.getHours()) + ":" + pad2(d.getMinutes());
}
function sameCalDay(a: number, b: number) {
  const x = new Date(a * 1000);
  const y = new Date(b * 1000);
  return x.getFullYear() === y.getFullYear() && x.getMonth() === y.getMonth() && x.getDate() === y.getDate();
}
function daySpan(a: number, b: number) {
  const x = new Date(a * 1000);
  const y = new Date(b * 1000);
  const d1 = new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const d2 = new Date(y.getFullYear(), y.getMonth(), y.getDate()).getTime();
  return Math.max(1, Math.round((d2 - d1) / 86400000) + 1);
}
// 事件页的副标题：同一天给时间弧，跨天给「M/D HH:MM → M/D HH:MM · N 天」
const spanLine = computed(() => {
  const a = active.value;
  if (!a || !isEvent.value) return "";
  if (sameCalDay(a.ts, a.endTs)) return timeOf(a.ts) + " – " + timeOf(a.endTs);
  const s = parts(a.ts);
  const e = parts(a.endTs);
  return s.m + "/" + pad2(s.d) + " " + timeOf(a.ts) + " → " + e.m + "/" + pad2(e.d) + " " + timeOf(a.endTs) + " · " + daySpan(a.ts, a.endTs) + " 天";
});
function pageLabel(p: Page) {
  const a = parts(p.ts);
  const b = parts(p.endTs);
  if (a.y === b.y && a.m === b.m && a.d === b.d) return a.m + " 月 " + a.d + " 日";
  return a.m + "/" + pad2(a.d) + " – " + b.m + "/" + pad2(b.d);
}
function setGroupMode(mode: "day" | "event") {
  if (mode === groupMode.value) return;
  const anchor = active.value?.ts ?? 0;
  groupMode.value = mode;
  try { localStorage.setItem(GROUP_MODE_KEY, mode); } catch { /* 忽略 */ }
  const list = days.value;
  let i = list.findIndex((p) => anchor >= p.ts && anchor <= p.endTs);
  if (i < 0) i = Math.min(activeIdx.value, list.length - 1);
  activeIdx.value = Math.max(0, i);
  if (mode === "event") emit("toast", "按拍摄间隔成页：超过 6 小时断开；跨午夜连续拍仍算一段。换手机不会断开。");
}

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
  const a = active.value;
  const p = activeParts.value;
  if (!a || !p) return "";
  if (sameCalDay(a.ts, a.endTs)) return p.m + " 月 " + p.d + " 日 · " + p.w;
  const e = parts(a.endTs);
  return p.m + "/" + pad2(p.d) + " – " + e.m + "/" + pad2(e.d) + " · " + daySpan(a.ts, a.endTs) + " 天";
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
  if (tourOpen.value && tourStep.value === 2) nextTick(advanceTour);
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
    emit("toast", "已经是最后一页了");
    return;
  }
  activeIdx.value = next;
  nextTick(() => {
    if (stripEl.value) stripEl.value.scrollLeft = 0;
  });
}
function jump(key: string) {
  const i = days.value.findIndex((d) => d.key === key);
  if (i < 0) return;
  const same = i === activeIdx.value;
  activeIdx.value = i;
  // 点的是当前这一天时 activeIdx 不变化，tour 也要能往前走
  if (same && tourOpen.value && tourStep.value === 0) nextTick(advanceTour);
}
function skipDay() {
  if (activeIdx.value >= days.value.length - 1) emit("toast", "这是最后一页，显影后这一轮就收尾了");
  else activeIdx.value++;
}
const wait = (ms: number) => new Promise<void>((r) => window.setTimeout(r, ms));

function stepVisible(i: number) {
  const el = root.value?.querySelector<HTMLElement>(TOUR_STEPS[i].sel);
  if (!el) return false;
  const b = el.getBoundingClientRect();
  return b.width > 0 && b.height > 0;
}

function measureTour() {
  if (!tourOpen.value) return;
  const step = TOUR_STEPS[tourStep.value];
  const el = root.value?.querySelector<HTMLElement>(step.sel);
  if (!el) { tourRect.value = null; return; }
  const b = el.getBoundingClientRect();
  if (!b.width || !b.height) { tourRect.value = null; return; }
  tourRect.value = {
    left: Math.max(4, b.left - 8),
    top: Math.max(4, b.top - 8),
    width: Math.min(window.innerWidth - 8, b.width + 16),
    height: b.height + 16,
  };
}

function startTour() {
  if (loading.value) { tourPending.value = true; return; }
  if (!days.value.length) { emit("toast", "这一轮已经挑完了，没有可演示的页"); return; }
  if (tourOpen.value) {
    document.removeEventListener("keydown", onTourKey, true);
    window.removeEventListener("resize", measureTour);
  }
  const first = TOUR_STEPS.findIndex((_, i) => stepVisible(i));
  if (first < 0) return;
  tourStep.value = first;
  tourOpen.value = true;
  document.addEventListener("keydown", onTourKey, true);
  window.addEventListener("resize", measureTour);
  nextTick(measureTour);
}

function advanceTour() {
  if (!tourOpen.value) return;
  let next = tourStep.value + 1;
  while (next < TOUR_STEPS.length && !stepVisible(next)) next += 1;
  if (next >= TOUR_STEPS.length) { finishTour(); return; }
  tourStep.value = next;
  nextTick(measureTour);
}

function finishTour() {
  tourOpen.value = false;
  tourRect.value = null;
  try { localStorage.setItem(TOUR_KEY, "done"); } catch { /* 忽略 */ }
  document.removeEventListener("keydown", onTourKey, true);
  window.removeEventListener("resize", measureTour);
}

function onTourKey(e: KeyboardEvent) {
  if (!tourOpen.value) return;
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    finishTour();
  }
}

const tourRingStyle = computed(() => {
  const r = tourRect.value;
  return r ? { left: r.left + "px", top: r.top + "px", width: r.width + "px", height: r.height + "px" } : {};
});
const tourAtTop = computed(() => tourStep.value >= 2);

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
    if (tourPending.value) {
      tourPending.value = false;
      if (days.value.length) nextTick(() => startTour());
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
    await Promise.all([api.developPage(keepIds, restIds), wait(140)]);
    rememberPage(day, keepIds);
    // 页面淡出后重读待挑 + 已保留（精选册由 kept 推导，撤销后自动回退）
    leaving.value = true;
    await wait(240);
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

// 跟着做一步，就自动往前走
watch(activeIdx, () => {
  if (tourOpen.value && tourStep.value === 0) advanceTour();
});
watch(() => developed.value.length, (n, o) => {
  if (tourOpen.value && tourStep.value === 3 && n > o) finishTour();
});

onMounted(async () => {
  await load();
  root.value?.focus({ preventScroll: true });
  let seen = "";
  try { seen = localStorage.getItem(TOUR_KEY) ?? ""; } catch { /* 忽略 */ }
  if (seen !== "done" && days.value.length) {
    tourTimer = window.setTimeout(() => { if (!tourOpen.value) startTour(); }, 900);
  }
});

onUnmounted(() => {
  window.clearTimeout(tourTimer);
  document.removeEventListener("keydown", onTourKey, true);
  window.removeEventListener("resize", measureTour);
});

defineExpose({ startTour });
</script>

<template>
  <section class="story" :class="{ dragging }" ref="root" tabindex="0" @keydown="onKey" aria-label="故事线挑图">
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
          <span class="tl-count">{{ days.length }} {{ groupUnit }}</span>
        </div>
        <div class="group-switch" role="group" aria-label="成页方式">
          <button :class="{ on: groupMode === 'day' }" :aria-pressed="groupMode === 'day'" @click="setGroupMode('day')">按天</button>
          <button
            :class="{ on: groupMode === 'event' }"
            :aria-pressed="groupMode === 'event'"
            title="按拍摄间隔成页：超过 6 小时算新的一段；跨午夜连续拍摄留在同一段；换手机拍同一场景不会断开"
            @click="setGroupMode('event')"
          >按事件</button>
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
              <span class="tl-week">{{ isEvent ? timeOf(d.ts) : parts(d.ts).w }}</span>
              <span class="tl-n">{{ d.items.length }}</span>
            </button>
          </template>
        </div>
        <div class="tl-progress">
          <i :style="{ width: donePct + '%' }"></i>
        </div>
        <div class="tl-foot">
          <span class="tl-foot-k">已显影</span>
          <span class="tl-foot-n"><b>{{ developed.length }}</b><i>/ {{ doneDays }} {{ groupShort }}</i></span>
        </div>
      </aside>

      <main class="stage">
        <!-- 窄屏换天：时间轴收成一条可横滑的「日签」，功能不消失 -->
        <nav class="day-rail" aria-label="按页翻页">
          <div class="rail-switch" role="group" aria-label="成页方式">
            <button :class="{ on: groupMode === 'day' }" :aria-pressed="groupMode === 'day'" @click="setGroupMode('day')">按天</button>
            <button :class="{ on: groupMode === 'event' }" :aria-pressed="groupMode === 'event'" @click="setGroupMode('event')">按事件</button>
          </div>
          <button
            v-for="d in days"
            :key="d.key"
            class="rail-day"
            :class="{ on: active && active.key === d.key }"
            @click="jump(d.key)"
          >
            <b>{{ parts(d.ts).m }}/{{ pad2(parts(d.ts).d) }}</b>
            <span>{{ isEvent ? timeOf(d.ts) : parts(d.ts).w }} · {{ d.items.length }}</span>
          </button>
        </nav>

        <div v-if="active && activeParts" class="stage-inner" :key="active.key">
          <header class="day-head">
            <div class="day-title">
              <span class="day-num">{{ pad2(activeParts.d) }}</span>
              <span class="day-sub">
                <b class="day-month">{{ activeParts.m }} 月</b>
                <span class="day-week">{{ activeParts.y }} · {{ activeParts.w }}</span>
                <span v-if="isEvent" class="day-span">{{ spanLine }}</span>
              </span>
            </div>
            <div class="day-meta">
              <span>{{ active.items.length }} 张待挑</span>
              <span class="mdot"></span>
              <span v-if="burstGroups">{{ burstGroups }} 组连拍</span>
              <span v-if="burstGroups" class="mdot"></span>
              <span>{{ chosen.length }} 张入选</span>
              <span class="hdr-kept">已显影 {{ developed.length }}</span>
              <span class="pagemark">P. {{ pad2(folio) }}</span>
            </div>
          </header>

          <div class="page-area">
            <div class="spread-wrap">
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
              </div>
            </div>

            <div class="compose">
              <div class="compose-count">
                <span class="cc-num" :key="currentBudget">{{ currentBudget }}</span>
                <div class="cc-text">
                  <span class="cc-k">这一页，留几张？</span>
                  <span class="cc-sub">{{ pickWhy || '按住刻度左右滑，页面会实时重排' }}</span>
                </div>
              </div>
              <div class="ticks-wrap">
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
              <span class="strip-title">{{ groupNoun }} · {{ active.items.length }} 张 <b class="strip-picked">已选 {{ chosen.length }}</b></span>
              <span class="strip-hint"><kbd>←</kbd><kbd>→</kbd> 张数 · 点照片加入 · 拖进页面 · <kbd>R</kbd> 重置</span>
            </div>
            <div class="strip" ref="stripEl" @wheel.prevent="onWheel">
              <button
                v-for="(m, idx) in active.items"
                :key="m.id"
                class="frame"
                :class="{ picked: pickedIds.has(m.id), banned: bannedIds.has(m.id), 'drag-src': dragId === m.id }"
                :title="frameTitle(m) + (pickedIds.has(m.id) ? ' · 已选（点一下移出）' : ' · 点一下加入，或直接拖进页面')"
                draggable="true"
                @click="togglePic(m)"
                @dragstart="onDragStart($event, m)"
                @dragend="onDragEnd"
              >
                <img :src="thumbUrl(m)" alt="" loading="lazy" draggable="false" />
                <span class="f-no">{{ pad2(idx + 1) }}</span>
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
              <i class="ghost-mark"></i>
            </div>
            <p class="ghost-copy">这一页还空着<br /><b>显影</b> 之后，收进第 1 页</p>
            <span class="ghost-hint">P. 01 · 会显示缩略样张</span>
          </div>
          <div
            v-for="(p, i) in developed"
            :key="p.key"
            class="mini"
            :style="{ '--i': i }"
          >
            <div class="mini-grid" :data-n="p.thumbs.length">
              <img v-for="(t, j) in p.thumbs" :key="j" :src="t" alt="" draggable="false" />
            </div>
            <div class="mini-foot">
              <span>{{ pageLabel(p) }}</span>
              <b>{{ p.count }} 张</b>
            </div>
          </div>
        </div>
        <button class="album-export" @click="emit('export')">
          <Icon name="export" :size="14" /> 导出精选集
        </button>
      </aside>
    </template>

    <Teleport to="body">
      <div v-if="tourOpen && tourRect" class="tour-layer">
        <div class="tour-ring" :style="tourRingStyle"></div>
        <div class="tour-card" :class="{ 'at-top': tourAtTop }" role="dialog" aria-label="上手引导">
          <div class="tour-kicker">
            上手 {{ tourStep + 1 }} / {{ TOUR_STEPS.length }}
            <span>· 做一步，它会自己往前走</span>
          </div>
          <h3>{{ TOUR_STEPS[tourStep].title }}</h3>
          <p>{{ TOUR_STEPS[tourStep].body }}</p>
          <div class="tour-actions">
            <button class="tour-skip" @click="finishTour">先不看</button>
            <span class="spacer"></span>
            <button class="tour-next" @click="advanceTour">
              {{ tourStep === TOUR_STEPS.length - 1 ? "知道了" : "下一步" }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<style scoped>
.story {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  background:
    radial-gradient(58% 46% at 46% -4%, oklch(from var(--accent) l c h / .09), transparent 68%),
    radial-gradient(120% 80% at 50% 118%, oklch(8% .02 150 / .5), transparent 62%);
  outline: none;
}
/* 底片颗粒：全页唯一一层「材质」，不参与布局与命中 */
.story::after {
  content: "";
  position: absolute;
  inset: 0;
  z-index: 5;
  background-image: var(--paper-grain);
  mix-blend-mode: overlay;
  opacity: .34;
  pointer-events: none;
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
  width: 190px;
  flex: none;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--border-1);
  background: color-mix(in oklab, var(--bg-0) 52%, transparent);
}
.tl-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 22px 18px 12px;
  border-bottom: 1px solid var(--border-1);
  margin-bottom: 4px;
}
.tl-kicker {
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: .2em;
  color: var(--text-4);
  font-weight: 500;
}
.tl-count { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); font-variant-numeric: tabular-nums; letter-spacing: .04em; }
.group-switch {
  display: flex;
  gap: 2px;
  margin: 9px 10px 7px;
  padding: 2px;
  border: 1px solid var(--border-1);
  border-radius: 7px;
  background: color-mix(in oklab, var(--bg-1) 55%, transparent);
}
.group-switch button {
  flex: 1;
  height: 22px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-4);
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: .12em;
  transition: background var(--dur-2) var(--ease-out), color var(--dur-2) var(--ease-out);
}
.group-switch button:hover { color: var(--text-2); }
.group-switch button.on { background: var(--accent-soft); color: var(--accent-ink); }
.tl-scroll { flex: 1; overflow-y: auto; padding: 0 9px 12px; }
.tl-month {
  position: sticky;
  top: 0;
  z-index: 1;
  padding: 13px 9px 6px;
  font-family: var(--font-mono);
  font-size: 9px;
  color: var(--text-4);
  background: linear-gradient(color-mix(in oklab, var(--bg-0) 92%, transparent) 62%, transparent);
  letter-spacing: .16em;
  backdrop-filter: blur(6px);
}
.tl-day {
  width: 100%;
  display: flex;
  align-items: baseline;
  gap: 9px;
  height: 36px;
  padding: 0 9px 0 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-2);
  font-size: var(--fs-2);
  transition: background var(--dur-2) var(--ease-out), color var(--dur-2) var(--ease-out), box-shadow var(--dur-2) var(--ease-out);
}
.tl-day:hover { background: var(--hover); color: var(--text-1); }
.tl-day.on {
  background: var(--accent-soft);
  color: var(--text-1);
  box-shadow: inset 1.5px 0 0 var(--accent);
}
.tl-dot {
  align-self: center;
  width: calc(4px + var(--w, 0) * 7px);
  height: calc(4px + var(--w, 0) * 7px);
  border-radius: 50%;
  background: var(--text-4);
  flex: none;
  transition: background var(--dur-2) var(--ease-out), box-shadow var(--dur-2) var(--ease-out);
}
.tl-day.on .tl-dot { background: var(--accent); box-shadow: 0 0 0 3px oklch(from var(--accent) l c h / .12); }
.tl-date { font-variant-numeric: tabular-nums; font-family: var(--font-mono); font-size: 10.5px; letter-spacing: -.02em; }
.tl-week { color: var(--text-4); font-size: 10px; }
.tl-n {
  margin-left: auto;
  align-self: center;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.tl-day.on .tl-n { color: var(--accent-ink); }
.tl-progress {
  flex: none;
  height: 1.5px;
  margin: 0 18px;
  border-radius: 2px;
  background: var(--border-1);
  overflow: hidden;
}
.tl-progress i {
  display: block;
  height: 100%;
  background: var(--accent);
  box-shadow: 0 0 8px oklch(from var(--accent) l c h / .6);
  transition: width var(--dur-4) var(--ease-out);
}
.tl-foot {
  flex: none;
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 8px;
  padding: 14px 18px 16px;
  border-top: 1px solid var(--border-1);
}
.tl-foot-k { font-size: 10px; letter-spacing: .1em; color: var(--text-4); padding-bottom: 2px; }
.tl-foot-n { display: flex; align-items: baseline; gap: 4px; }
.tl-foot b {
  font-family: var(--font-display);
  font-weight: 400;
  font-size: 22px;
  line-height: .9;
  color: var(--text-1);
  font-variant-numeric: lining-nums;
}
.tl-foot i { font-style: normal; font-family: var(--font-mono); font-size: 9.5px; color: var(--text-4); letter-spacing: .04em; }

/* ── 中：今日成页 ── */
.stage {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 20px 34px 14px;
  display: flex;
  flex-direction: column;
}
.stage-inner {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 14px;
  min-height: 0;
  animation: dayIn var(--dur-4) var(--ease-spring) both;
}
.page-area {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 22px;
}
@keyframes dayIn {
  from { opacity: 0; transform: translateY(14px); }
  to { opacity: 1; transform: none; }
}

/* 窄屏「日签」：默认不出现，920 以下接手时间轴的换天职责 */
.day-rail { display: none; }

.day-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 24px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-1);
}
.day-title { display: flex; align-items: flex-end; gap: 14px; }
.day-num {
  font-family: var(--font-display);
  font-size: var(--fs-day);
  font-weight: 400;
  line-height: .78;
  letter-spacing: -.035em;
  font-variant-numeric: lining-nums tabular-nums;
  color: var(--text-1);
}
.day-sub {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding-bottom: 7px;
}
.day-month {
  font-family: var(--font-display);
  font-weight: 400;
  font-size: 18px;
  line-height: 1.1;
  letter-spacing: .01em;
  color: var(--text-2);
}
.day-week {
  font-family: var(--font-mono);
  font-size: 10px;
  letter-spacing: .14em;
  color: var(--text-3);
}
.day-span {
  margin-top: 2px;
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: .06em;
  color: var(--accent-ink);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.day-meta {
  display: flex;
  align-items: baseline;
  gap: 9px;
  padding-bottom: 2px;
  font-family: var(--font-mono);
  font-size: 10px;
  letter-spacing: .07em;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.hdr-kept { display: none; }
.pagemark {
  padding-left: 10px;
  margin-left: 2px;
  border-left: 1px solid var(--border-1);
  color: var(--text-3);
  letter-spacing: .12em;
}
.mdot { width: 2.5px; height: 2.5px; border-radius: 50%; background: var(--text-4); }

.spread-wrap {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 0;
  animation: pageTurn 340ms var(--ease-out) both;
}
/* 相纸底下的一团暖光：像把它放在灯箱上看 */
.spread-wrap::before {
  content: "";
  grid-area: 1 / 1;
  z-index: 0;
  align-self: stretch;
  justify-self: stretch;
  margin: 4% 2%;
  background: radial-gradient(52% 52% at 50% 46%, oklch(from var(--accent) l c h / .14), transparent 72%);
  filter: blur(28px);
  pointer-events: none;
}
@keyframes pageTurn {
  from { opacity: 0; transform: translateY(10px) rotate(-.4deg); }
  to { opacity: 1; transform: none; }
}
.spread {
  position: relative;
  grid-area: 1 / 1;
  z-index: 1;
  width: min(100%, calc((100vh - 372px) * 1.3333), 900px);
  min-width: 320px;
  aspect-ratio: 4 / 3;
  padding: 16px 16px 9px;
  border-radius: 2px;
  background-color: var(--paper);
  background-image: var(--paper-grain);
  background-size: 180px 180px;
  background-blend-mode: multiply;
  color: var(--paper-ink);
  box-shadow:
    inset 0 1px 0 oklch(100% 0 0 / .7),
    0 0 0 .5px var(--paper-edge),
    0 2px 4px -2px oklch(0% 0 0 / .5),
    0 12px 26px -18px oklch(8% .02 150 / .62),
    0 44px 80px -46px oklch(6% .02 150 / .66);
  display: flex;
  flex-direction: column;
  transition: transform var(--dur-4) var(--ease-spring), box-shadow var(--dur-4) var(--ease-out);
}
/* 四角印刷角标：只在相纸留白里，不压照片 */
.spread::before {
  content: "";
  position: absolute;
  inset: 6px;
  pointer-events: none;
  color: oklch(35% .02 90 / .3);
  background:
    linear-gradient(currentColor, currentColor) left top / 9px 1px no-repeat,
    linear-gradient(currentColor, currentColor) left top / 1px 9px no-repeat,
    linear-gradient(currentColor, currentColor) right top / 9px 1px no-repeat,
    linear-gradient(currentColor, currentColor) right top / 1px 9px no-repeat,
    linear-gradient(currentColor, currentColor) left bottom / 9px 1px no-repeat,
    linear-gradient(currentColor, currentColor) left bottom / 1px 9px no-repeat,
    linear-gradient(currentColor, currentColor) right bottom / 9px 1px no-repeat,
    linear-gradient(currentColor, currentColor) right bottom / 1px 9px no-repeat;
}
.spread.developing { transform: scale(.988) translateY(2px); }
.spread.developing::after {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: radial-gradient(90% 72% at 50% 44%, oklch(100% 0 0 / .5), transparent 72%);
  animation: developFlash 300ms var(--ease-out) both;
}
@keyframes developFlash {
  0% { opacity: 0; }
  32% { opacity: 1; }
  100% { opacity: 0; }
}
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
  transform: translateY(-5px) scale(.992);
  transition:
    transform 220ms var(--ease-out),
    opacity 200ms var(--ease-out);
}
.slots {
  flex: 1;
  min-height: 0;
  display: grid;
  gap: 5px;
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
/* 照片压进纸面的一线内阴影，避免「贴纸」感 */
.slot::after {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  box-shadow: inset 0 0 0 1px oklch(0% 0 0 / .1), inset 0 1px 3px oklch(0% 0 0 / .12);
}
.slot img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform var(--dur-4) var(--ease-out), filter var(--dur-2) var(--ease-out);
  /* 翻到这一天时，照片像在显影液里慢慢浮出来 */
  animation: photoDevelop 860ms var(--ease-out) backwards;
  animation-delay: calc(var(--i, 0) * 70ms + 50ms);
}
@keyframes photoDevelop {
  0% { opacity: .35; filter: blur(11px) saturate(.32) brightness(1.14); }
  55% { opacity: 1; filter: blur(2.5px) saturate(.82) brightness(1.03); }
  100% { opacity: 1; filter: none; }
}
.slot:hover img { transform: scale(1.02); filter: brightness(.9); }
.slot-tag {
  position: absolute;
  left: 7px;
  top: 7px;
  padding: 2.5px 6px 2px;
  border-radius: 2px;
  background: oklch(12% .01 80 / .55);
  color: oklch(97% .006 90);
  font-size: 8.5px;
  font-family: var(--font-mono);
  letter-spacing: .12em;
  backdrop-filter: blur(6px);
  pointer-events: none;
  z-index: 1;
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
  padding: 8px 1px 0;
  font-family: var(--font-display);
  font-size: 10.5px;
  line-height: 1.2;
  color: var(--paper-ink-2);
  letter-spacing: .06em;
}
.folio { font-family: var(--font-mono); font-size: 8.5px; letter-spacing: .14em; color: var(--paper-ink-2); }

.slot-enter-active { transition: opacity var(--dur-3) var(--ease-out), transform var(--dur-4) var(--ease-spring); transition-delay: calc(var(--i, 0) * 30ms); }
.slot-leave-active { transition: opacity var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-out); position: absolute; }
.slot-enter-from { opacity: 0; transform: scale(.94) translateY(10px); }
.slot-leave-to { opacity: 0; transform: scale(.96); }
.slot-move { transition: transform var(--dur-4) var(--ease-spring); }

.compose {
  display: flex;
  align-items: center;
  gap: 24px;
}
.compose-count { display: flex; align-items: center; gap: 12px; min-width: 200px; }
.cc-num {
  font-family: var(--font-display);
  font-size: 42px;
  font-weight: 400;
  line-height: .82;
  letter-spacing: -.02em;
  color: var(--text-1);
  font-variant-numeric: lining-nums tabular-nums;
  min-width: 30px;
  animation: numIn 260ms var(--ease-spring) both;
}
@keyframes numIn {
  from { opacity: 0; transform: translateY(5px) scale(.96); }
  to { opacity: 1; transform: none; }
}
.cc-text { display: flex; flex-direction: column; gap: 4px; }
.cc-k { font-size: 14px; font-weight: 600; color: var(--text-1); letter-spacing: .005em; }
.cc-sub { font-size: 10.5px; color: var(--text-3); white-space: nowrap; }
.ticks-wrap { flex: 1; display: flex; justify-content: center; min-width: 0; }
/* 刻度尺：可点、可拖，选中的数字下方有一枚「曝光标记」 */
.ticks {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: 0;
  width: 204px;
  padding: 2px 0 0;
  touch-action: none;
  cursor: ew-resize;
  user-select: none;
}
.ticks-line {
  position: absolute;
  left: 14px;
  right: 14px;
  top: 26px;
  height: 5px;
  background:
    linear-gradient(var(--border-2), var(--border-2)) 0 0 / 100% 1px no-repeat,
    repeating-linear-gradient(90deg, var(--border-1) 0 1px, transparent 1px 8.5px) 0 100% / 100% 4px no-repeat;
}
.ticks button {
  position: relative;
  width: 34px;
  height: 34px;
  padding: 2px 0 0;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text-4);
  font-family: var(--font-mono);
  font-size: 10px;
  letter-spacing: .02em;
  font-variant-numeric: tabular-nums;
  transition: color var(--dur-2) var(--ease-out), transform var(--dur-1) var(--ease-out);
}
.ticks button i {
  position: absolute;
  left: 50%;
  bottom: 4px;
  width: 4px;
  height: 4px;
  margin-left: -2px;
  border-radius: 50%;
  background: var(--border-2);
  transition: background var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-spring), box-shadow var(--dur-2) var(--ease-out);
}
.ticks button:hover { color: var(--text-1); }
.ticks button:hover i { background: var(--text-3); }
.ticks button.lit { color: var(--text-2); }
.ticks button.lit i { background: var(--accent); opacity: .6; }
.ticks button.at { color: var(--text-1); font-weight: 600; transform: translateY(-2px); }
.ticks button.at i {
  background: var(--accent);
  transform: scale(1.6);
  box-shadow: 0 0 0 3px var(--accent-soft), 0 0 12px oklch(from var(--accent) l c h / .45);
}
.ticks button:active { transform: translateY(0) scale(.94); }
.ticks button:disabled { opacity: .25; }
.compose-actions { display: flex; align-items: center; gap: 8px; margin-left: auto; }
.develop {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  height: 46px;
  padding: 0 20px;
  border: none;
  border-radius: var(--r-pill);
  background: linear-gradient(180deg, oklch(from var(--accent) calc(l + .045) c h), var(--accent));
  color: var(--on-accent);
  font-size: 13.5px;
  font-weight: 650;
  letter-spacing: .04em;
  box-shadow:
    inset 0 1px 0 oklch(100% 0 0 / .3),
    0 12px 26px -14px oklch(from var(--accent) l c h / .8);
  transition: transform var(--dur-1) var(--ease-out), background var(--dur-2) var(--ease-out), box-shadow var(--dur-2) var(--ease-out);
}
.develop:hover:not(:disabled) {
  background: linear-gradient(180deg, var(--accent-strong), oklch(from var(--accent) calc(l - .02) c h));
  transform: translateY(-1px);
  box-shadow:
    inset 0 1px 0 oklch(100% 0 0 / .34),
    0 16px 32px -14px oklch(from var(--accent) l c h / .9);
}
.develop:active { transform: translateY(0) scale(.98); }
.develop:disabled { opacity: .38; box-shadow: none; }
.develop kbd {
  display: inline-grid;
  place-items: center;
  min-width: 17px;
  height: 17px;
  padding: 0 4px;
  border-radius: 3px;
  background: oklch(from var(--on-accent) l c h / .16);
  color: inherit;
  font-size: 9.5px;
  line-height: 1;
}
.skip { height: 46px; padding: 0 10px; color: var(--text-3); letter-spacing: .02em; }
.skip:hover { color: var(--text-2); }
.skip kbd { margin-left: 2px; }

.strip-wrap { flex: none; position: relative; }
.strip-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 2px 6px;
  font-size: 10.5px;
  letter-spacing: .02em;
  color: var(--text-4);
}
.strip-title { display: inline-flex; align-items: baseline; gap: 8px; color: var(--text-3); font-size: 11px; }
.strip-hint { display: inline-flex; align-items: center; gap: 4px; color: var(--text-4); }
.strip {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  overflow-y: hidden;
  padding: 10px 2px 12px;
  scrollbar-width: thin;
  scroll-behavior: smooth;
  /* 右缘渐隐：暗示还能继续往后翻，不用画滚动条 */
  mask-image: linear-gradient(90deg, #000 0, #000 96%, transparent 100%);
}
.strip-picked {
  margin-left: 8px;
  padding: 1.5px 7px 1px;
  border-radius: var(--r-pill);
  background: var(--accent-soft);
  color: var(--accent-ink, var(--accent));
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: .06em;
  font-weight: 500;
}
.frame {
  position: relative;
  flex: none;
  width: 96px;
  height: 70px;
  padding: 0;
  border: none;
  border-radius: 4px;
  overflow: hidden;
  background: var(--thumb-bg);
  opacity: .58;
  transform: translateY(0);
  box-shadow: 0 0 0 .5px oklch(100% 0 0 / .07);
  transition: opacity var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-spring), box-shadow var(--dur-2) var(--ease-out);
}
.frame img { width: 100%; height: 100%; object-fit: cover; display: block; }
.frame:hover { opacity: 1; transform: translateY(-2px); box-shadow: 0 6px 16px -10px oklch(0% 0 0 / .7); }
.frame:active { cursor: grabbing; }
.frame.picked {
  opacity: 1;
  transform: translateY(-4px);
  box-shadow: 0 0 0 2px var(--accent), 0 14px 24px -14px oklch(0% 0 0 / .7);
}
.frame.picked:hover { transform: translateY(-5px); }
.frame.banned { opacity: .26; }
/* 拖动时：整条胶片退后，被拖的那一格反亮 */
.story.dragging .frame { opacity: .3; }
.story.dragging .frame.drag-src { opacity: .9; transform: translateY(-3px) scale(.98); }
.f-no {
  position: absolute;
  left: 5px;
  bottom: 4px;
  font-family: var(--font-mono);
  font-size: 8.5px;
  line-height: 1;
  letter-spacing: .06em;
  color: oklch(100% 0 0 / .72);
  text-shadow: 0 1px 3px oklch(0% 0 0 / .75);
  pointer-events: none;
}
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
  width: 240px;
  flex: none;
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--border-1);
  background: color-mix(in oklab, var(--bg-0) 52%, transparent);
}
.album-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 22px 18px 12px;
}
.album-kicker {
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: .2em;
  color: var(--text-4);
  font-weight: 500;
}
.album-count { font-family: var(--font-mono); font-size: 10px; color: var(--text-3); font-variant-numeric: tabular-nums; letter-spacing: .04em; }
.album-scroll { flex: 1; overflow-y: auto; padding: 2px 18px 12px; display: flex; flex-direction: column; gap: 14px; }
.album-empty { display: grid; justify-items: center; gap: 14px; padding: 26px 0 18px; margin: auto 0; text-align: center; }
.album-empty b { color: var(--accent); font-weight: 600; }
.ghost-page {
  position: relative;
  width: 120px;
  aspect-ratio: 4 / 3;
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-rows: 1fr 1fr;
  gap: 2px;
  padding: 7px;
  border-radius: 2px;
  background-color: var(--paper);
  background-image: var(--paper-grain);
  background-size: 180px 180px;
  background-blend-mode: multiply;
  box-shadow: 0 12px 24px -18px oklch(0% 0 0 / .7), 0 0 0 .5px var(--paper-edge);
  transform: rotate(-.7deg);
}
.ghost-page span { background: oklch(90% .01 95); border-radius: 1px; }
.ghost-page span:first-child { background: oklch(87% .018 160); }
.ghost-mark {
  position: absolute;
  inset: 4px;
  pointer-events: none;
  color: oklch(35% .02 90 / .28);
  background:
    linear-gradient(currentColor, currentColor) left top / 7px 1px no-repeat,
    linear-gradient(currentColor, currentColor) left top / 1px 7px no-repeat,
    linear-gradient(currentColor, currentColor) right top / 7px 1px no-repeat,
    linear-gradient(currentColor, currentColor) right top / 1px 7px no-repeat,
    linear-gradient(currentColor, currentColor) left bottom / 7px 1px no-repeat,
    linear-gradient(currentColor, currentColor) left bottom / 1px 7px no-repeat,
    linear-gradient(currentColor, currentColor) right bottom / 7px 1px no-repeat,
    linear-gradient(currentColor, currentColor) right bottom / 1px 7px no-repeat;
}
.ghost-copy {
  font-family: var(--font-display);
  font-size: 14px;
  line-height: 1.75;
  color: var(--text-3);
}
.ghost-hint { font-family: var(--font-mono); font-size: 9px; letter-spacing: .1em; color: var(--text-4); }
.mini {
  background-color: var(--paper);
  background-image: var(--paper-grain);
  background-size: 180px 180px;
  background-blend-mode: multiply;
  padding: 6px 6px 5px;
  border-radius: 2px;
  color: var(--paper-ink);
  box-shadow: 0 10px 22px -18px oklch(0% 0 0 / .8), 0 0 0 .5px var(--paper-edge);
  animation: pageIn 300ms var(--ease-out) both;
  /* 一叠相纸：交替微小偏转，hover 时回正抬起 */
  transition: transform var(--dur-2) var(--ease-spring), box-shadow var(--dur-2) var(--ease-out);
}
.mini:nth-child(odd) { transform: rotate(-.5deg); }
.mini:nth-child(even) { transform: rotate(.42deg); }
.mini:hover { transform: rotate(0deg) translateY(-2px); box-shadow: 0 16px 28px -18px oklch(0% 0 0 / .9), 0 0 0 .5px var(--paper-edge); }
@keyframes pageIn {
  from { opacity: 0; translate: 0 6px; }
  to { opacity: 1; translate: 0 0; }
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
  padding: 6px 1px 1px;
  font-family: var(--font-display);
  font-size: 10.5px;
  letter-spacing: .06em;
  color: var(--paper-ink-2);
}
.mini-foot b { font-family: var(--font-mono); font-size: 8.5px; letter-spacing: .1em; color: var(--paper-ink-2); }
.album-export {
  flex: none;
  margin: 0 14px 14px;
  height: 38px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  border: 1px solid var(--border-1);
  border-radius: var(--r-pill);
  background: transparent;
  color: var(--text-2);
  font-size: 11.5px;
  letter-spacing: .04em;
}
.album-export:hover { border-color: oklch(from var(--accent) l c h / .45); color: var(--text-1); background: var(--hover); }

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

/* ── 首次运行：跟着做一遍 ── */
.tour-layer { position: fixed; inset: 0; z-index: 300; pointer-events: none; }
.tour-ring {
  position: fixed;
  border-radius: 10px;
  pointer-events: none;
  box-shadow:
    0 0 0 9999px oklch(0% 0 0 / .5),
    0 0 0 1.5px var(--accent),
    0 18px 50px -16px oklch(0% 0 0 / .6);
  transition:
    left .28s var(--ease-out),
    top .28s var(--ease-out),
    width .28s var(--ease-out),
    height .28s var(--ease-out);
}
/* 引导卡也做成一张相纸：和右侧「精选册」同一套物质语言 */
.tour-card {
  position: fixed;
  left: 50%;
  bottom: 22px;
  transform: translateX(-50%);
  width: min(474px, calc(100vw - 32px));
  padding: 18px 20px 15px;
  border: none;
  border-radius: 3px;
  background-color: var(--paper);
  background-image: var(--paper-grain);
  background-size: 180px 180px;
  background-blend-mode: multiply;
  color: var(--paper-ink);
  box-shadow:
    inset 0 1px 0 oklch(100% 0 0 / .7),
    0 0 0 .5px var(--paper-edge),
    0 26px 60px -26px oklch(0% 0 0 / .8);
  pointer-events: auto;
  z-index: 301;
  animation: tourIn 240ms var(--ease-out) both;
}
.tour-card::before {
  content: "";
  position: absolute;
  inset: 7px;
  pointer-events: none;
  color: oklch(35% .02 90 / .3);
  background:
    linear-gradient(currentColor, currentColor) left top / 8px 1px no-repeat,
    linear-gradient(currentColor, currentColor) left top / 1px 8px no-repeat,
    linear-gradient(currentColor, currentColor) right top / 8px 1px no-repeat,
    linear-gradient(currentColor, currentColor) right top / 1px 8px no-repeat,
    linear-gradient(currentColor, currentColor) left bottom / 8px 1px no-repeat,
    linear-gradient(currentColor, currentColor) left bottom / 1px 8px no-repeat,
    linear-gradient(currentColor, currentColor) right bottom / 8px 1px no-repeat,
    linear-gradient(currentColor, currentColor) right bottom / 1px 8px no-repeat;
}
.tour-card.at-top { top: 64px; bottom: auto; }
@keyframes tourIn {
  from { opacity: 0; transform: translate(-50%, 8px) rotate(-.3deg); }
  to { opacity: 1; transform: translate(-50%, 0); }
}
.tour-kicker {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-family: var(--font-mono);
  font-size: 9.5px;
  letter-spacing: .14em;
  color: oklch(45% .12 145);
}
.tour-kicker span {
  font-family: var(--font-body);
  font-size: 10px;
  letter-spacing: .02em;
  color: var(--paper-ink-2);
}
.tour-card h3 {
  margin: 9px 0 6px;
  font-family: var(--font-display);
  font-size: 21px;
  font-weight: 400;
  line-height: 1.25;
  letter-spacing: .01em;
}
.tour-card p {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.75;
  color: oklch(40% .012 80);
}
.tour-actions { display: flex; align-items: center; gap: 10px; margin-top: 16px; }
.tour-skip {
  height: 30px;
  padding: 0 2px;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--paper-ink-2);
  font-size: 11.5px;
  cursor: pointer;
}
.tour-skip:hover { color: var(--paper-ink); }
.tour-actions .spacer { flex: 1; }
.tour-next {
  height: 32px;
  padding: 0 15px;
  border: none;
  border-radius: 3px;
  background: oklch(30% .06 150);
  color: var(--paper);
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: .04em;
  cursor: pointer;
  box-shadow: inset 0 1px 0 oklch(100% 0 0 / .14);
}
.tour-next:hover { background: oklch(26% .06 150); }

@media (prefers-reduced-motion: reduce) {
  .story::after { display: none; }
  .spread-wrap, .slot img, .cc-num, .mini, .tour-card { animation: none; }
  .spread.developing::after { animation: none; opacity: 0; }
  .slots, .slot, .frame, .mini, .spread { transition: none; }
  .tour-ring { transition: none; }
}

@media (max-width: 1240px) {
  .album { width: 208px; }
  .stage { padding: 18px 26px 12px; }
}
@media (max-width: 1120px) {
  .album { display: none; }
  .hdr-kept { display: inline; }
}
@media (max-width: 920px) {
  .timeline { display: none; }
  .day-rail {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 0 0 4px;
    margin-bottom: 4px;
    scrollbar-width: none;
  }
  .day-rail::-webkit-scrollbar { display: none; }
  .rail-switch {
    position: sticky;
    left: 0;
    z-index: 2;
    flex: none;
    display: flex;
    gap: 2px;
    align-self: center;
    height: 40px;
    padding: 3px;
    margin-right: 2px;
    border: 1px solid var(--border-1);
    border-radius: 6px;
    background: var(--bg-0);
  }
  .rail-switch button {
    padding: 0 9px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-4);
    font-family: var(--font-mono);
    font-size: 9.5px;
    letter-spacing: .1em;
  }
  .rail-switch button.on { background: var(--accent-soft); color: var(--accent-ink); }
  .rail-day {
    flex: none;
    display: flex;
    align-items: baseline;
    gap: 7px;
    height: 40px;
    padding: 0 12px;
    border: 1px solid var(--border-1);
    border-radius: 6px;
    background: transparent;
    color: var(--text-3);
    transition: background var(--dur-2) var(--ease-out), color var(--dur-2) var(--ease-out), border-color var(--dur-2) var(--ease-out);
  }
  .rail-day b {
    font-family: var(--font-mono);
    font-size: 10.5px;
    font-weight: 500;
    letter-spacing: .02em;
    font-variant-numeric: tabular-nums;
  }
  .rail-day span { font-size: 9px; color: var(--text-4); letter-spacing: .04em; }
  .rail-day.on {
    background: var(--accent-soft);
    color: var(--text-1);
    border-color: oklch(from var(--accent) l c h / .45);
    box-shadow: inset 0 0 0 1px oklch(from var(--accent) l c h / .2);
  }
  .rail-day.on span { color: var(--accent-ink, var(--accent)); }
  .stage { padding: 16px 18px 12px; }
  /* 窄屏少了左栏，但顶栏 + 日签 + compose + 胶片条占得更多：按剩余高度定纸宽 */
  .spread { width: min(100%, calc((100vh - 440px) * 1.3333)); min-width: 0; }
  .compose-count { min-width: 0; }
  .cc-sub { display: none; }
  .strip-hint { display: none; }
}
@media (max-width: 760px) {
  .stage { padding: 14px 14px 10px; }
  .day-num { font-size: 54px; }
  .day-title { gap: 10px; }
  .day-meta { gap: 7px; font-size: 9px; }
  .pagemark { display: none; }
  .compose { flex-wrap: wrap; row-gap: 8px; gap: 16px; }
  .ticks-wrap { order: 3; flex-basis: 100%; justify-content: flex-start; }
  .compose-actions { margin-left: auto; }
  .develop { height: 42px; padding: 0 16px; }
  .skip { height: 42px; }
}
@media (max-width: 560px) {
  .day-week { display: none; }
  .strip-head { display: none; }
  .cc-num { font-size: 34px; }
  .album-export { margin: 0 10px 10px; }
}
</style>
