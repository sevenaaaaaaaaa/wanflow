<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import type { Cmd } from "../types";

const props = defineProps<{ open: boolean; commands: Cmd[] }>();
const emit = defineEmits<{ close: [] }>();

const q = ref("");
const idx = ref(0);
const input = ref<HTMLInputElement | null>(null);

const filtered = computed(() => {
  const s = q.value.trim().toLowerCase();
  if (!s) return props.commands;
  return props.commands.filter(
    (c) => c.label.toLowerCase().includes(s) || c.group.toLowerCase().includes(s),
  );
});

const grouped = computed(() => {
  const out: { group: string; items: Cmd[] }[] = [];
  for (const c of filtered.value) {
    const g = out.find((x) => x.group === c.group);
    if (g) g.items.push(c);
    else out.push({ group: c.group, items: [c] });
  }
  return out;
});

const flat = computed(() => grouped.value.flatMap((g) => g.items));

watch(
  () => props.open,
  async (v) => {
    if (!v) return;
    q.value = "";
    idx.value = 0;
    await nextTick();
    input.value?.focus();
  },
);

watch(q, () => (idx.value = 0));

function move(d: number) {
  const n = flat.value.length;
  if (!n) return;
  idx.value = (idx.value + d + n) % n;
}

function choose(c?: Cmd) {
  const cmd = c ?? flat.value[idx.value];
  if (!cmd) return;
  emit("close");
  cmd.run();
}

function onKey(e: KeyboardEvent) {
  if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
  else if (e.key === "Enter") { e.preventDefault(); choose(); }
  else if (e.key === "Escape") { e.preventDefault(); emit("close"); }
}

function indexOf(c: Cmd) {
  return flat.value.indexOf(c);
}
</script>

<template>
  <div v-if="open" class="palette-mask" @mousedown.self="emit('close')">
    <div class="palette" @keydown="onKey">
      <input
        ref="input"
        v-model="q"
        type="text"
        placeholder="输入命令…（↑↓ 选择，↵ 执行，esc 关闭）"
        spellcheck="false"
      />
      <div class="list">
        <template v-for="g in grouped" :key="g.group">
          <div class="group">{{ g.group }}</div>
          <div
            v-for="c in g.items"
            :key="c.id"
            class="item"
            :class="{ on: indexOf(c) === idx }"
            @mouseenter="idx = indexOf(c)"
            @click="choose(c)"
          >
            <span>{{ c.label }}</span>
            <span v-if="c.hint" class="hint">{{ c.hint }}</span>
            <span v-if="c.kbd" class="k">{{ c.kbd }}</span>
          </div>
        </template>
        <div v-if="!flat.length" class="empty">没有匹配的命令</div>
      </div>
    </div>
  </div>
</template>