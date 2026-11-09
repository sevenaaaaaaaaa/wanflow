<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type TagItem } from "../api";
import Icon from "./Icon.vue";

const props = defineProps<{ ids: number[] }>();
const emit = defineEmits<{
  close: [];
  applied: [label: string];
}>();

const tags = ref<TagItem[]>([]);
const draft = ref("");
const busy = ref(false);
const error = ref("");

async function load() {
  try {
    tags.value = await api.listTags();
  } catch (e) {
    error.value = String(e);
  }
}

async function assign(name: string, remove: boolean) {
  const clean = name.trim();
  if (!clean || busy.value || !props.ids.length) return;
  busy.value = true;
  error.value = "";
  try {
    const label = await api.assignTags(props.ids, clean, remove);
    if (!remove) draft.value = "";
    emit("applied", label);
    await load();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <div class="modal-mask" @click.self="emit('close')">
    <div class="modal tag-modal" role="dialog" aria-modal="true" aria-labelledby="tag-title">
      <header>
        <h2 id="tag-title">打标签</h2>
        <span class="count">{{ props.ids.length }} 张</span>
        <button class="icon-btn close" aria-label="关闭" @click="emit('close')">
          <Icon name="x" :size="14" />
        </button>
      </header>

      <div class="body">
        <div class="new-row">
          <input
            v-model="draft"
            type="text"
            maxlength="24"
            placeholder="新建或复用标签，比如：春天、周岁"
            @keyup.enter="assign(draft, false)"
          />
          <button
            class="primary"
            :disabled="!draft.trim() || busy || !props.ids.length"
            @click="assign(draft, false)"
          >
            打上
          </button>
        </div>

        <p v-if="!tags.length" class="hint">
          还没有标签。标签是横跨相册和时间的分组，同一张照片可以有多个。
        </p>
        <ul v-else class="tag-list">
          <li v-for="t in tags" :key="t.id">
            <span class="tag-row">
              <button class="pick" :disabled="busy" @click="assign(t.name, false)">
                <Icon name="tag" :size="13" />
                <span class="name">{{ t.name }}</span>
                <span class="n">{{ t.count }}</span>
              </button>
              <button
                class="remove"
                :disabled="busy"
                :aria-label="`从选中的照片上移除「${t.name}」`"
                @click="assign(t.name, true)"
              >
                移除
              </button>
            </span>
          </li>
        </ul>

        <p v-if="error" class="err">{{ error }}</p>
        <p class="note">打上和移除都是一次动作，可用 ⌘Z 整体撤销。</p>
      </div>

      <footer>
        <button class="ghost" @click="emit('close')">完成</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.tag-modal {
  width: min(520px, 92vw);
  max-height: 72vh;
}
header {
  display: flex;
  align-items: baseline;
  gap: var(--sp-2);
  padding: var(--sp-4) var(--sp-5);
  border-bottom: 0.5px solid var(--border-1);
  font-size: var(--fs-4);
  font-weight: 500;
}
header h2 {
  font-size: var(--fs-4);
  font-weight: 500;
  color: var(--text-1);
}
header .count {
  color: var(--text-3);
  font-size: var(--fs-2);
  font-family: var(--font-mono);
}
header .close {
  margin-left: auto;
}
.body {
  padding: var(--sp-4) var(--sp-5);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.hint,
.note {
  color: var(--text-3);
  font-size: var(--fs-2);
}
.tag-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
  padding: 0;
  margin: 0;
}
.tag-row {
  display: flex;
  align-items: center;
  gap: var(--sp-1);
}
.pick {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3);
  border: 0.5px solid transparent;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text-2);
  text-align: left;
}
.pick:hover {
  background: var(--hover);
  color: var(--text-1);
}
.pick .name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pick .n {
  color: var(--text-3);
  font-size: var(--fs-2);
  font-family: var(--font-mono);
}
.remove {
  height: 24px;
  padding: 0 var(--sp-2);
  border: 0.5px solid transparent;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text-3);
  font-size: var(--fs-2);
}
.remove:hover {
  border-color: var(--border-1);
  color: var(--danger-ink);
}
.new-row {
  display: flex;
  gap: var(--sp-2);
  align-items: center;
}
.new-row input {
  flex: 1;
  min-width: 0;
}
.err {
  color: var(--danger-ink);
  font-size: var(--fs-2);
}
footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--sp-2);
  padding: var(--sp-3) var(--sp-5);
  border-top: 0.5px solid var(--border-1);
}
</style>
