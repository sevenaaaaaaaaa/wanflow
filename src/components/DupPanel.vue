<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, fmtBytes, fmtTime, thumbUrl, type MediaItem } from "../api";

const props = defineProps<{ group: number }>();
const emit = defineEmits<{ close: []; decide: [d: "keep" | "reject" | "pending"]; refresh: [] }>();

const members = ref<MediaItem[]>([]);
const picked = ref<Set<number>>(new Set());
const loading = ref(true);

onMounted(async () => {
  members.value = await api.dupMembers(props.group);
  loading.value = false;
});

function toggle(id: number) {
  picked.value.has(id) ? picked.value.delete(id) : picked.value.add(id);
}
async function decide(d: "keep" | "reject" | "pending") {
  const ids = [...picked.value];
  if (ids.length) {
    await api.setDecision(ids, d, null);
    picked.value.clear();
    members.value = await api.dupMembers(props.group);
    emit("refresh");
  }
}
async function keepOne(id: number) {
  // 组内留这张，其余标为待复阅的"重复-淘汰"
  const others = members.value.filter(m => m.id !== id).map(m => m.id);
  await api.setDecision([id], "keep", null);
  if (others.length) await api.setDecision(others, "reject", null);
  members.value = await api.dupMembers(props.group);
  emit("refresh");
}
</script>

<template>
  <div class="dup-panel">
    <header>
      重复组 #{{ group }} · {{ members.length }} 张
      <span class="spacer"></span>
      <button @click="$emit('close')">✕</button>
    </header>
    <div class="dup-list">
      <p v-if="loading" class="hint">加载中…</p>
      <p v-else class="hint" style="padding: 0 var(--sp-4) var(--sp-6)">
        内容相同或高度相似。挑一张最好的"只留这张"，其余自动标为淘汰。
      </p>
      <div v-for="m in members" :key="m.id" class="dup-card" :class="{ selected: picked.has(m.id) }">
        <img v-if="m.thumb" :src="thumbUrl(m)" @click="toggle(m.id)" style="cursor: pointer" />
        <div v-else style="width: 74px; height: 74px; display: grid; place-items: center; color: var(--text-4); font-size: 11px; border: 0.5px solid var(--border-1); border-radius: var(--r-sm)">无缩略图</div>
        <div class="info">
          <b :title="m.path">{{ m.filename }}</b>
          {{ fmtTime(m.effTime) }} · {{ fmtBytes(m.sizeBytes) }}
          <template v-if="m.width"> · {{ m.width }}×{{ m.height }}</template>
          <br />
          <span v-if="m.decision" :style="{ color: m.decision === 'keep' ? 'var(--keep)' : m.decision === 'reject' ? 'var(--reject)' : 'var(--pending)' }">
            {{ m.decision === "keep" ? "已保留" : m.decision === "reject" ? "已淘汰" : "待定" }}
          </span>
          <span v-else>未处理</span>
        </div>
        <button class="primary" style="padding: var(--sp-5) var(--sp-10)" @click="keepOne(m.id)">只留这张</button>
      </div>
    </div>
    <div style="padding: var(--sp-10) var(--sp-14); border-top: 0.5px solid var(--border-1); display: flex; gap: var(--sp-8)">
      <button class="primary" :disabled="picked.size === 0" @click="decide('keep')">选中保留</button>
      <button :disabled="picked.size === 0" style="color: var(--danger-ink)" @click="decide('reject')">选中淘汰</button>
      <span class="spacer"></span>
      <span class="hint">{{ picked.size }} 张选中</span>
    </div>
  </div>
</template>
