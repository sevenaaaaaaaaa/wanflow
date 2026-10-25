<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { api, fmtBytes, fmtTime, previewUrl, type MediaItem } from "../api";

const props = defineProps<{ group: number }>();
const emit = defineEmits<{ close: []; refresh: [] }>();

const members = ref<MediaItem[]>([]);
const pos = ref(0);
const busy = ref(false);
const done = ref(false);

onMounted(async () => {
  document.addEventListener("keydown", onKey);
  members.value = (await api.burstMembers(props.group)).filter(m => m.kind === "image");
  if (members.value.length < 2) done.value = true;
});
onUnmounted(() => document.removeEventListener("keydown", onKey));

const left = computed(() => members.value[pos.value]);
const right = computed(() => members.value[pos.value + 1]);
const totalPairs = computed(() => Math.max(0, members.value.length - 1));

async function act(kind: "left" | "right" | "both" | "skip") {
  if (busy.value || !left.value || !right.value) return;
  busy.value = true;
  try {
    if (kind === "left") {
      await api.setDecision([left.value.id], "keep", null);
      await api.setDecision([right.value.id], "reject", null);
    } else if (kind === "right") {
      await api.setDecision([right.value.id], "keep", null);
      await api.setDecision([left.value.id], "reject", null);
    } else if (kind === "both") {
      await api.setDecision([left.value.id, right.value.id], "keep", null);
    }
    pos.value += 1;
    if (pos.value >= members.value.length - 1) done.value = true;
    emit("refresh");
  } finally {
    busy.value = false;
  }
}

// 键盘：←/→ 选边，B 都留，S 跳过
function onKey(e: KeyboardEvent) {
  if (done.value) return;
  if (e.key === "ArrowLeft") act("left");
  else if (e.key === "ArrowRight") act("right");
  else if (e.key.toLowerCase() === "b") act("both");
  else if (e.key.toLowerCase() === "s") act("skip");
}

const metrics = (m: MediaItem) => [
  { label: "清晰", v: m.sharpness },
  { label: "曝光", v: m.exposure },
  { label: "综合", v: m.quality },
];

// 一句话差异分析（本地指标版；M4 接 VLM 后升级为画面级解释）
const verdict = computed(() => {
  if (!left.value || !right.value) return "";
  const parts: string[] = [];
  const s1 = left.value.sharpness ?? 0;
  const s2 = right.value.sharpness ?? 0;
  const e1 = left.value.exposure ?? 0;
  const e2 = right.value.exposure ?? 0;
  if (Math.abs(s1 - s2) >= 8) {
    const win = s1 > s2 ? "左" : "右";
    parts.push(`${win}图更清晰（${Math.max(s1, s2).toFixed(0)} 对 ${Math.min(s1, s2).toFixed(0)}）`);
  }
  if (Math.abs(e1 - e2) >= 8) {
    const win = e1 > e2 ? "左" : "右";
    parts.push(`${win}图曝光更好（${Math.max(e1, e2).toFixed(0)} 对 ${Math.min(e1, e2).toFixed(0)}）`);
  }
  return parts.length ? parts.join("；") : "两张指标接近，凭表情和构图自己定";
});
</script>

<template>
  <div class="arena-mask" @click.self="$emit('close')">
    <div class="arena">
      <header class="arena-head">
        <b>擂台 · 连拍组 #{{ group }}</b>
        <span class="hint">{{ done ? "比完了" : `第 ${pos + 1}/${totalPairs} 组` }}</span>
        <span class="spacer"></span>
        <button @click="$emit('close')">完成</button>
      </header>

      <div v-if="!done && left && right" class="arena-stage">
        <div v-for="(m, side) in [left, right]" :key="m.id" class="arena-card" :class="side === 0 ? 'is-left' : 'is-right'">
          <img :src="previewUrl(m)" draggable="false" />
          <div class="arena-meta">
            <b>{{ side === 0 ? "← 左" : "右 →" }}</b>
            <span>{{ fmtTime(m.effTime) }} · {{ fmtBytes(m.sizeBytes) }}</span>
            <div class="arena-bars">
              <div v-for="bar in metrics(m)" :key="bar.label" class="arena-bar">
                <span>{{ bar.label }}</span>
                <div class="track"><div class="fill" :style="{ width: (bar.v ?? 0) + '%' }"></div></div>
                <em>{{ (bar.v ?? 0).toFixed(0) }}</em>
              </div>
            </div>
          </div>
        </div>
      </div>

      <p v-if="!done && left && right" class="arena-verdict">💡 {{ verdict }}</p>

      <div v-if="!done && left && right" class="arena-actions">
        <button class="primary" @click="act('left')">留左（←）</button>
        <button class="primary" @click="act('right')">留右（→）</button>
        <button @click="act('both')">都留（B）</button>
        <button @click="act('skip')">跳过（S）</button>
      </div>

      <div v-if="done" class="arena-done">
        <div class="logo">🏆</div>
        <p>这组比完了，赢家已标"保留"，其余标"淘汰"（可在历史撤销）。</p>
        <button class="primary" @click="$emit('close')">好的</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.arena-mask {
  position: fixed;
  inset: 0;
  background: rgba(5, 6, 10, 0.9);
  z-index: 120;
  display: flex;
  align-items: center;
  justify-content: center;
  animation: fadeIn 0.18s ease;
}
.arena {
  width: min(1080px, 94vw);
  max-height: 92vh;
  background: var(--bg2);
  border: 1px solid var(--line);
  border-radius: 16px;
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  animation: popIn 0.22s cubic-bezier(0.2, 0.9, 0.3, 1.15);
}
.arena-head { display: flex; align-items: center; gap: 10px; }
.arena-stage {
  display: flex;
  gap: 16px;
  justify-content: center;
  align-items: flex-start;
}
.arena-card {
  flex: 1;
  max-width: 46%;
  border-radius: 12px;
  overflow: hidden;
  background: var(--bg3);
  border: 2px solid var(--line);
  animation: popIn 0.25s cubic-bezier(0.2, 0.9, 0.3, 1.15);
}
.arena-card.is-left { animation-name: slideL; }
.arena-card.is-right { animation-name: slideR; }
@keyframes slideL { from { transform: translateX(-40px); opacity: 0; } to { transform: none; opacity: 1; } }
@keyframes slideR { from { transform: translateX(40px); opacity: 0; } to { transform: none; opacity: 1; } }
.arena-card img {
  width: 100%;
  max-height: 46vh;
  object-fit: contain;
  background: #10131c;
  display: block;
}
.arena-meta { padding: 10px 12px; display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--muted); }
.arena-meta b { color: var(--text); font-size: 14px; }
.arena-bars { display: flex; flex-direction: column; gap: 4px; margin-top: 4px; }
.arena-bar { display: flex; align-items: center; gap: 8px; font-size: 11px; }
.arena-bar .track {
  flex: 1;
  height: 6px;
  border-radius: 3px;
  background: #12151f;
  overflow: hidden;
}
.arena-bar .fill {
  height: 100%;
  background: linear-gradient(90deg, var(--accent), #a78bfa);
  transition: width 0.4s cubic-bezier(0.2, 0.9, 0.3, 1);
}
.arena-bar em { font-style: normal; min-width: 24px; text-align: right; color: var(--text); }
.arena-verdict { text-align: center; color: var(--text); font-size: 13.5px; background: var(--bg3); border-radius: 10px; padding: 8px 14px; }
.arena-actions { display: flex; gap: 10px; justify-content: center; }
.arena-actions button { font-size: 14px; padding: 9px 18px; }
.arena-done { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 30px 0 16px; }
</style>
