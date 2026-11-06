<script setup lang="ts">
import { computed, h, onMounted, onUnmounted, ref, watch } from "vue";
import { thumbUrl, type MediaItem } from "../api";

const props = defineProps<{
  items: MediaItem[];
  total: number;
  loading: boolean;
  selection: Set<number>;
}>();
const IconCheck = () => h("svg", { width: 13, height: 13, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", "stroke-width": 2.4, "stroke-linecap": "round", "stroke-linejoin": "round" }, [h("path", { d: "M2.8 8.4l3.2 3.2 7.2-7.2" })]);
const IconX = () => h("svg", { width: 13, height: 13, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", "stroke-width": 2.2, "stroke-linecap": "round" }, [h("path", { d: "M3.6 3.6l8.8 8.8M12.4 3.6l-8.8 8.8" })]);
const IconStar = () => h("svg", { width: 13, height: 13, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", "stroke-width": 1.6, "stroke-linejoin": "round" }, [h("path", { d: "M8 2.2l1.83 3.85 4.17.56-3.05 2.9.77 4.16L8 11.7l-3.72 1.97.77-4.16L2 6.61l4.17-.56z" })]);

const emit = defineEmits<{
  toggle: [id: number];
  open: [idx: number];
  dup: [group: number];
  arena: [group: number];
  decide: [ids: number[], decision: "keep" | "reject" | "pending"];
  star: [ids: number[], stars: number];
  range: [ids: number[]];
}>();

// —— 自实现虚拟网格 ——
const scrollEl = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportH = ref(600);
const GAP = 10;
const COL_W = ref(210);

const cols = computed(() =>
  Math.max(1, Math.floor(((scrollEl.value?.clientWidth ?? 1200) - GAP) / (COL_W.value + GAP))),
);
const rowH = computed(() => COL_W.value * 0.78 + GAP);
const rowCount = computed(() => Math.ceil(props.items.length / cols.value));

const startRow = computed(() => Math.max(0, Math.floor(scrollTop.value / rowH.value) - 2));
const endRow = computed(() =>
  Math.min(rowCount.value, Math.ceil((scrollTop.value + viewportH.value) / rowH.value) + 2),
);

const visible = computed(() => {
  const out: { idx: number; m: MediaItem; x: number; y: number }[] = [];
  for (let r = startRow.value; r < endRow.value; r++) {
    for (let c = 0; c < cols.value; c++) {
      const idx = r * cols.value + c;
      if (idx >= props.items.length) break;
      out.push({ idx, m: props.items[idx], x: c * (COL_W.value + GAP), y: r * rowH.value });
    }
  }
  return out;
});

function onScroll() {
  if (!scrollEl.value) return;
  scrollTop.value = scrollEl.value.scrollTop;
  viewportH.value = scrollEl.value.clientHeight;
  COL_W.value = Math.max(
    150,
    Math.min(260, (scrollEl.value.clientWidth - GAP) / Math.max(3, Math.floor(scrollEl.value.clientWidth / 230))),
  );
}
let ro: ResizeObserver | null = null;
onMounted(() => {
  ro = new ResizeObserver(onScroll);
  if (scrollEl.value) ro.observe(scrollEl.value);
  onScroll();
  // 键盘优先：进来就能用方向键
  scrollEl.value?.focus({ preventScroll: true });
});
onUnmounted(() => ro?.disconnect());

// —— 焦点游标（键盘优先） ——
const focusIdx = ref(0);
const anchor = ref(0);

watch(
  () => props.items.length,
  (n) => {
    if (focusIdx.value > n - 1) focusIdx.value = Math.max(0, n - 1);
    if (anchor.value > n - 1) anchor.value = Math.max(0, n - 1);
  },
);

function scrollIntoView(idx: number) {
  const el = scrollEl.value;
  if (!el) return;
  const row = Math.floor(idx / cols.value);
  const top = row * rowH.value;
  const bottom = top + rowH.value;
  if (top < el.scrollTop + 8) el.scrollTop = Math.max(0, top - 12);
  else if (bottom > el.scrollTop + el.clientHeight - 8) el.scrollTop = bottom - el.clientHeight + 12;
}

function moveFocus(next: number, extend: boolean) {
  const n = props.items.length;
  if (!n) return;
  const idx = Math.max(0, Math.min(n - 1, next));
  focusIdx.value = idx;
  scrollIntoView(idx);
  if (extend) {
    const a = Math.min(anchor.value, idx);
    const b = Math.max(anchor.value, idx);
    emit("range", props.items.slice(a, b + 1).map((m) => m.id));
  } else {
    anchor.value = idx;
  }
  scrollEl.value?.focus({ preventScroll: true });
}

function onKey(e: KeyboardEvent) {
  if (props.items.length === 0) return;
  const c = cols.value;
  const page = Math.max(1, Math.floor(viewportH.value / rowH.value));
  const cur = focusIdx.value;
  switch (e.key) {
    case "ArrowLeft": e.preventDefault(); moveFocus(cur - 1, e.shiftKey); break;
    case "ArrowRight": e.preventDefault(); moveFocus(cur + 1, e.shiftKey); break;
    case "ArrowUp": e.preventDefault(); moveFocus(cur - c, e.shiftKey); break;
    case "ArrowDown": e.preventDefault(); moveFocus(cur + c, e.shiftKey); break;
    case "Home": e.preventDefault(); moveFocus(0, e.shiftKey); break;
    case "End": e.preventDefault(); moveFocus(props.items.length - 1, e.shiftKey); break;
    case "PageUp": e.preventDefault(); moveFocus(cur - c * page, e.shiftKey); break;
    case "PageDown": e.preventDefault(); moveFocus(cur + c * page, e.shiftKey); break;
    case "Enter": e.preventDefault(); emit("open", cur); break;
    case " ": {
      e.preventDefault();
      const m = props.items[cur];
      if (m) emit("toggle", m.id);
      anchor.value = cur;
      break;
    }
    case "Escape":
      e.preventDefault();
      emit("range", []);
      break;
    case "p": case "P": emit("decide", [props.items[cur].id], "keep"); break;
    case "x": case "X": emit("decide", [props.items[cur].id], "reject"); break;
    case "u": case "U": emit("decide", [props.items[cur].id], "pending"); break;
    default:
      if (/^[0-5]$/.test(e.key)) emit("star", [props.items[cur].id], Number(e.key));
  }
}

// —— 点击选中 / 双击灯箱 ——
let lastClick = { idx: -1, t: 0 };
function cellClick(idx: number, m: MediaItem, e: MouseEvent) {
  focusIdx.value = idx;
  scrollEl.value?.focus({ preventScroll: true });
  const now = Date.now();
  if (lastClick.idx === idx && now - lastClick.t < 350) {
    emit("open", idx);
  } else if (e.shiftKey) {
    const a = Math.min(anchor.value, idx);
    const b = Math.max(anchor.value, idx);
    emit("range", props.items.slice(a, b + 1).map((x) => x.id));
  } else {
    emit("toggle", m.id);
    anchor.value = idx;
  }
  lastClick = { idx, t: now };
}

const heightStyle = computed(() => ({ height: `${rowCount.value * rowH.value}px` }));

function scoreClass(q: number) {
  if (q >= 70) return "good";
  if (q >= 45) return "mid";
  return "low";
}

defineExpose({ focusFirst: () => moveFocus(0, false) });
</script>

<template>
  <div
    class="grid-scroll"
    ref="scrollEl"
    tabindex="0"
    @scroll.passive="onScroll"
    @keydown="onKey"
  >
    <div v-if="loading && items.length === 0" class="empty">
      <p>加载中…</p>
    </div>
    <div v-else-if="items.length === 0" class="empty">
      <p>当前过滤条件下没有照片</p>
    </div>
    <div class="grid-inner" :style="heightStyle" v-else>
      <div
        v-for="c in visible"
        :key="c.m.id"
        class="cell"
        :class="{ selected: selection.has(c.m.id), focused: c.idx === focusIdx }"
        :style="{
          width: COL_W + 'px',
          height: COL_W * 0.78 + 'px',
          transform: `translate(${c.x}px, ${c.y}px)`,
        }"
        :title="c.m.filename"
        @click="cellClick(c.idx, c.m, $event)"
      >
        <img v-if="c.m.thumb" :src="thumbUrl(c.m)" loading="lazy" draggable="false" />
        <div v-else class="thumbfallback" aria-hidden="true"></div>

        <div class="badge">
          <span v-if="c.m.decision" class="state" :class="c.m.decision">
            {{ c.m.decision === "keep" ? "留" : c.m.decision === "reject" ? "弃" : "疑" }}
          </span>
          <span v-if="c.m.stars" class="state stars">★{{ c.m.stars }}</span>
          <span v-if="c.m.kind === 'video'" class="state">视频</span>
        </div>

        <div class="meta">
          <span v-if="c.m.burstBest === 1" class="tag best" title="AI 选的连拍最佳">最佳</span>
          <span v-else-if="c.m.burstGroup !== null" class="tag" title="进擂台比一比" @click.stop="emit('arena', c.m.burstGroup!)">连拍</span>
          <span v-if="c.m.dupGroup !== null" class="tag" title="查看重复组" @click.stop="emit('dup', c.m.dupGroup!)">重复</span>
        </div>

        <span v-if="c.m.quality !== null" class="score" :class="scoreClass(c.m.quality)">{{ Math.round(c.m.quality) }}</span>

        <div class="quick" @click.stop>
          <button class="cellbtn keep" title="保留 (P)" @click="emit('decide', [c.m.id], 'keep')">
            <IconCheck />
          </button>
          <button class="cellbtn reject" title="淘汰 (X)" @click="emit('decide', [c.m.id], 'reject')">
            <IconX />
          </button>
          <button class="cellbtn star" title="加星" @click="emit('star', [c.m.id], Math.min(5, (c.m.stars || 0) + 1))">
            <IconStar />
          </button>
        </div>

        <div class="check"><IconCheck /></div>

      </div>
    </div>
  </div>
</template>

<style scoped>
.grid-scroll:focus { outline: none; }
.thumbfallback {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  font-size: 28px;
  color: var(--text-4);
}
</style>