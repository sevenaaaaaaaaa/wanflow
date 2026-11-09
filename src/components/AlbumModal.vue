<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type AlbumItem } from "../api";
import Icon from "./Icon.vue";

const props = defineProps<{ ids: number[] }>();
const emit = defineEmits<{
  close: [];
  applied: [label: string];
}>();

const albums = ref<AlbumItem[]>([]);
const picked = ref<number | null>(null);
const draft = ref("");
const busy = ref(false);
const error = ref("");

async function load() {
  try {
    albums.value = await api.listAlbums();
    if (picked.value === null && albums.value.length) picked.value = albums.value[0].id;
  } catch (e) {
    error.value = String(e);
  }
}

async function create() {
  const name = draft.value.trim();
  if (!name || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.createAlbum(name, props.ids);
    emit("applied", `已新建「${name}」，收进 ${props.ids.length} 张`);
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function add() {
  if (picked.value === null || busy.value) return;
  const album = albums.value.find((a) => a.id === picked.value);
  busy.value = true;
  error.value = "";
  try {
    const n = await api.addToAlbum(picked.value, props.ids);
    const skipped = props.ids.length - n;
    emit(
      "applied",
      skipped > 0
        ? `「${album?.name ?? "精选集"}」新增 ${n} 张，${skipped} 张原本就在里面`
        : `「${album?.name ?? "精选集"}」新增 ${n} 张`,
    );
    emit("close");
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
    <div class="modal album-modal" role="dialog" aria-modal="true" aria-labelledby="album-title">
      <header>
        <h2 id="album-title">加入精选集</h2>
        <span class="count">{{ props.ids.length }} 张</span>
        <button class="icon-btn close" aria-label="关闭" @click="emit('close')">
          <Icon name="x" :size="14" />
        </button>
      </header>

      <div class="body">
        <p v-if="!albums.length" class="hint">还没有精选集，先给这一组起个名字。</p>
        <ul v-else class="album-list">
          <li v-for="a in albums" :key="a.id">
            <button
              class="album-row"
              :class="{ on: picked === a.id }"
              :aria-pressed="picked === a.id"
              @click="picked = a.id"
            >
              <Icon name="layers" :size="14" />
              <span class="name">{{ a.name }}</span>
              <span class="n">{{ a.count }} 张</span>
            </button>
          </li>
        </ul>

        <div class="new-row">
          <input
            v-model="draft"
            type="text"
            maxlength="40"
            placeholder="新建精选集，比如：周岁"
            @keyup.enter="create"
          />
          <button :disabled="!draft.trim() || busy || !props.ids.length" @click="create">
            新建并收下
          </button>
        </div>

        <p v-if="error" class="err">{{ error }}</p>
      </div>

      <footer>
        <button class="ghost" @click="emit('close')">取消</button>
        <button
          class="primary"
          :disabled="picked === null || busy || !props.ids.length"
          @click="add"
        >
          加入
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.album-modal {
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
.hint {
  color: var(--text-3);
  font-size: var(--fs-2);
}
.album-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: var(--sp-1);
  padding: 0;
  margin: 0;
}
.album-row {
  width: 100%;
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
.album-row:hover {
  background: var(--hover);
  color: var(--text-1);
}
.album-row.on {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--text-1);
}
.album-row .name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.album-row .n {
  color: var(--text-3);
  font-size: var(--fs-2);
  font-family: var(--font-mono);
}
.new-row {
  display: flex;
  gap: var(--sp-2);
  align-items: center;
  padding-top: var(--sp-2);
  border-top: 0.5px solid var(--border-1);
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
