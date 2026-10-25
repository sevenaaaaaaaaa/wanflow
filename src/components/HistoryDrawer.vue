<script setup lang="ts">
import type { HistoryEntry } from "../api";

defineProps<{ entries: HistoryEntry[] }>();
defineEmits<{ close: []; undo: []; redo: [] }>();

function fmt(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}
</script>

<template>
  <div class="modal-mask" @click.self="$emit('close')">
    <div class="modal" style="width: min(560px, 92vw)">
      <header>
        操作历史（全部可撤销）
        <span class="spacer"></span>
        <button @click="$emit('undo')">撤销一步</button>
        <button @click="$emit('redo')">重做一步</button>
        <button @click="$emit('close')">✕</button>
      </header>
      <div class="body">
        <p v-if="entries.length === 0" class="hint">还没有操作记录</p>
        <div v-for="e in entries" :key="e.id" class="history-item" :class="{ undone: e.undone }">
          <span>{{ e.undone ? "○" : "●" }}</span>
          <span>{{ e.label }}</span>
          <span class="t">{{ fmt(e.createdAt) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
