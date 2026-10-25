<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { thumbUrl, type MediaItem } from "../api";

const props = defineProps<{
  items: MediaItem[];
  total: number;
  loading: boolean;
  selection: Set<number>;
}>();
const emit = defineEmits<{
  toggle: [id: number];
  open: [idx: number];
  dup: [group: number];
  arena: [group: number];
}>();

// —— 自实现虚拟网格 ——
const scrollEl = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportH = ref(600);
const GAP = 10;
const COL_W = ref(210);

const cols = computed(() =>
  Math.max(1, Math.floor(((scrollEl.value?.clientWidth ?? 1200) - GAP) / (COL_W.value + GAP)))
);
const rowH = computed(() => COL_W.value * 0.78 + GAP);
const rowCount = computed(() => Math.ceil(props.items.length / cols.value));

const startRow = computed(() => Math.max(0, Math.floor(scrollTop.value / rowH.value) - 2));
const endRow = computed(() =>
  Math.min(rowCount.value, Math.ceil((scrollTop.value + viewportH.value) / rowH.value) + 2)
);

const visible = computed(() => {
  const out: { idx: number; m: MediaItem; x: number; y: number }[] = [];
  for (let r = startRow.value; r < endRow.value; r++) {
    for (let c = 0; c < cols.value; c++) {
      const idx = r * cols.value + c;
      if (idx >= props.items.length) break;
      out.push({
        idx,
        m: props.items[idx],
        x: c * (COL_W.value + GAP),
        y: r * rowH.value,
      });
    }
  }
  return out;
});

function onScroll() {
  if (!scrollEl.value) return;
  scrollTop.value = scrollEl.value.scrollTop;
  viewportH.value = scrollEl.value.clientHeight;
  COL_W.value = Math.max(150, Math.min(260, (scrollEl.value.clientWidth - GAP) / Math.max(3, Math.floor(scrollEl.value.clientWidth / 230))));
}
let ro: ResizeObserver | null = null;
onMounted(() => {
  ro = new ResizeObserver(onScroll);
  if (scrollEl.value) ro.observe(scrollEl.value);
  onScroll();
});
onUnmounted(() => ro?.disconnect());

// 点击选中 / 双击灯箱
let lastClick = { idx: -1, t: 0 };
function cellClick(idx: number, m: MediaItem) {
  const now = Date.now();
  if (lastClick.idx === idx && now - lastClick.t < 350) {
    emit("open", idx); // 双击
  } else {
    emit("toggle", m.id);
  }
  lastClick = { idx, t: now };
}

const heightStyle = computed(() => ({ height: `${rowCount.value * rowH.value}px` }));

function scoreColor(q: number) {
  return q >= 70
    ? { background: "rgba(34,197,94,0.85)" }
    : q >= 45
      ? { background: "rgba(234,179,8,0.85)" }
      : { background: "rgba(239,68,68,0.85)" };
}
</script>

<template>
  <div class="grid-scroll" ref="scrollEl" @scroll.passive="onScroll">
    <div v-if="loading && items.length === 0" class="empty">
      <p>加载中…</p>
    </div>
    <div v-else-if="items.length === 0" class="empty">
      <div class="logo">🔎</div>
      <p>当前过滤条件下没有照片</p>
    </div>
    <div class="grid-inner" :style="heightStyle" v-else>
      <div
        v-for="c in visible"
        :key="c.m.id"
        class="cell"
        :class="{ selected: selection.has(c.m.id) }"
        :style="{
          width: COL_W + 'px',
          height: COL_W * 0.78 + 'px',
          transform: `translate(${c.x}px, ${c.y}px)`,
        }"
        @click="cellClick(c.idx, c.m)"
      >
        <img v-if="c.m.thumb" :src="thumbUrl(c.m)" loading="lazy" draggable="false" />
        <div v-else style="display:flex;align-items:center;justify-content:center;height:100%;color:var(--muted);font-size:28px">
          {{ c.m.kind === "video" ? "🎬" : "🖼️" }}
        </div>
        <div class="badge">
          <span v-if="c.m.decision" :class="c.m.decision">
            {{ c.m.decision === "keep" ? "留" : c.m.decision === "reject" ? "弃" : "疑" }}
          </span>
          <span v-if="c.m.stars" class="stars">★{{ c.m.stars }}</span>
          <span v-if="c.m.kind === 'video'" class="vid">视频</span>
          <span
            v-if="c.m.dupGroup !== null"
            class="dup"
            style="cursor: pointer"
            title="查看重复组"
            @click.stop="emit('dup', c.m.dupGroup!)"
            >重复</span
          >
          <span v-if="c.m.burstBest === 1" class="best" title="AI 选的连拍最佳">最佳</span>
          <span
            v-else-if="c.m.burstGroup !== null"
            class="dup"
            style="cursor: pointer"
            title="进擂台比一比"
            @click.stop="emit('arena', c.m.burstGroup!)"
            >连拍</span
          >
          <span v-if="c.m.quality !== null" class="score" :style="scoreColor(c.m.quality)">{{
            Math.round(c.m.quality)
          }}</span>
        </div>
        <div class="namebar">{{ c.m.filename }}</div>
      </div>
    </div>
  </div>
</template>
