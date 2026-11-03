<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  fmtBytes,
  type AlbumItem,
  type ExportOptions,
  type ExportPlan,
  type ExportSummary,
  type ScanStatus,
} from "../api";

const props = defineProps<{ ids: number[] }>();
const emit = defineEmits<{ close: []; done: [summary: ExportSummary] }>();

const scope = ref<"album" | "keeps" | "selection">(props.ids.length > 0 ? "selection" : "keeps");
const albums = ref<AlbumItem[]>([]);
const albumId = ref<number | null>(null);
const destDir = ref("");
const layout = ref<ExportOptions["layout"]>("flat");
const template = ref("{date}_{seq:04}_{orig}");
const stripMetadata = ref(false);
const rewriteTime = ref(false);
const includeVideos = ref(true);

const plan = ref<ExportPlan | null>(null);
const previewing = ref(false);
const running = ref(false);
const progress = ref<ScanStatus>({ running: false, phase: "", done: 0, total: 0, current: "" });
const error = ref("");
let unlisteners: (() => void)[] = [];

const TEMPLATES: { label: string; tpl: string }[] = [
  { label: "日期_序号_原名", tpl: "{date}_{seq:04}_{orig}" },
  { label: "日期_时间", tpl: "{date}_{time}" },
  { label: "精选集_序号", tpl: "{album}_{seq:04}" },
  { label: "原名（保留）", tpl: "{orig}" },
];

const opts = computed<ExportOptions | null>(() => {
  if (!destDir.value) return null;
  if (scope.value === "album" && !albumId.value) return null;
  if (scope.value === "selection" && props.ids.length === 0) return null;
  return {
    destDir: destDir.value,
    scope: scope.value,
    albumId: scope.value === "album" ? albumId.value ?? undefined : undefined,
    ids: scope.value === "selection" ? props.ids : undefined,
    layout: layout.value,
    nameTemplate: template.value,
    stripMetadata: stripMetadata.value,
    rewriteTime: rewriteTime.value,
    includeVideos: includeVideos.value,
  };
});

const pct = computed(() =>
  progress.value.total > 0 ? Math.round((progress.value.done / progress.value.total) * 100) : 0,
);

function relDest(row: { dest: string }) {
  const root = plan.value?.destDir ?? "";
  return row.dest.startsWith(root) ? row.dest.slice(root.length).replace(/^\//, "") : row.dest;
}

async function pickDir() {
  const picked = await open({ directory: true, title: "选择导出到哪个文件夹（原图不会被改动）" });
  if (!picked) return;
  destDir.value = String(picked);
  plan.value = null;
}

function pickStrip(v: boolean) {
  stripMetadata.value = v;
  if (v) rewriteTime.value = false;
  plan.value = null;
}
function pickRewrite(v: boolean) {
  rewriteTime.value = v;
  if (v) stripMetadata.value = false;
  plan.value = null;
}

function invalidate() {
  plan.value = null;
}

async function preview() {
  if (!opts.value) return;
  previewing.value = true;
  error.value = "";
  try {
    plan.value = await api.exportPreview(opts.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    previewing.value = false;
  }
}

async function apply() {
  if (!opts.value || !plan.value || plan.value.copyCount === 0) return;
  running.value = true;
  error.value = "";
  try {
    await api.exportApply(opts.value);
  } catch (e) {
    error.value = String(e);
    running.value = false;
  }
}

onMounted(async () => {
  albums.value = await api.listAlbums();
  if (albums.value.length > 0) albumId.value = albums.value[0].id;
  unlisteners = await Promise.all([
    listen<ScanStatus>("export://progress", e => (progress.value = e.payload)),
    listen<ExportSummary>("export://done", e => {
      const s = e.payload;
      running.value = false;
      if (s.copied > 0 || s.failed > 0 || s.skipped > 0) {
        emit("done", s);
        emit("close");
      }
    }),
    listen<string>("export://error", e => {
      error.value = e.payload;
      // 单个文件失败仍会继续，等 done 事件收尾
    }),
  ]);
});
onUnmounted(() => unlisteners.forEach(u => u()));
</script>

<template>
  <div class="modal-mask" @click.self="!running && $emit('close')">
    <div class="modal">
      <header>
        导出精选集（{{ props.ids.length }} 张已选中 · 只写副本，原图一个字节都不改）
        <span class="spacer"></span>
        <button :disabled="running" @click="$emit('close')">✕</button>
      </header>
      <div class="body">
        <div class="form-row">
          <label>导出范围</label>
          <label><input type="radio" value="album" v-model="scope" @change="invalidate" /> 精选集</label>
          <select v-if="scope === 'album'" v-model="albumId" @change="invalidate" style="width: 180px">
            <option v-if="albums.length === 0" :value="null">还没有精选集</option>
            <option v-for="a in albums" :key="a.id" :value="a.id">{{ a.name }}（{{ a.count }} 张）</option>
          </select>
          <label><input type="radio" value="keeps" v-model="scope" @change="invalidate" /> 所有保留</label>
          <label>
            <input type="radio" value="selection" v-model="scope" :disabled="props.ids.length === 0" @change="invalidate" />
            当前选中（{{ props.ids.length }}）
          </label>
        </div>

        <div class="form-row">
          <label>目标文件夹</label>
          <button @click="pickDir">选择目录…</button>
          <span class="hint" style="max-width: 420px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
            {{ destDir || "还没选" }}
          </span>
        </div>

        <div class="form-row">
          <label>目录结构</label>
          <select v-model="layout" @change="invalidate">
            <option value="flat">全部平铺</option>
            <option value="year">按年 / 2023</option>
            <option value="month">按年月 / 2023/10</option>
            <option value="album">按精选集名</option>
          </select>
          <label style="min-width: auto">命名</label>
          <input v-model="template" @input="invalidate" style="width: 240px" />
          <select @change="(e) => { template = (e.target as HTMLSelectElement).value; invalidate(); }" style="width: 150px">
            <option value="">套用模板…</option>
            <option v-for="t in TEMPLATES" :key="t.tpl" :value="t.tpl">{{ t.label }}</option>
          </select>
        </div>
        <div class="form-row">
          <span class="hint">
            可用占位符：{date} {time} {yyyy} {mm} {dd} {album} {seq:04} {orig} {stars}。
            文件名里的 / 等字符会被替换成 _，重名时自动加 _2 后缀。
          </span>
        </div>

        <div class="form-row">
          <label>元数据</label>
          <label><input type="checkbox" :checked="stripMetadata" @change="pickStrip(($event.target as HTMLInputElement).checked)" /> 剥离 EXIF（含 GPS）</label>
          <label><input type="checkbox" :checked="rewriteTime" @change="pickRewrite(($event.target as HTMLInputElement).checked)" /> 用修正后的时间改写 EXIF</label>
          <label><input type="checkbox" v-model="includeVideos" @change="invalidate" /> 包含视频</label>
        </div>
        <div class="form-row">
          <span class="hint">剥离与改写只对 JPEG 生效；HEIC/PNG/视频一律原样复制。</span>
        </div>

        <p v-if="error" style="color: #fca5a5; margin-bottom: 12px">{{ error }}</p>

        <div class="form-row">
          <button :disabled="!opts || previewing || running" @click="preview">
            {{ previewing ? "计算中…" : "预览 (dry-run)" }}
          </button>
          <button
            class="primary"
            :disabled="!plan || plan.copyCount === 0 || running"
            @click="apply"
          >
            {{ running ? "导出中…" : `导出 ${plan?.copyCount ?? 0} 个文件` }}
          </button>
          <span v-if="plan" class="hint">
            复制 {{ plan.copyCount }} 个（{{ fmtBytes(plan.copyBytes) }}） · 跳过 {{ plan.skipCount }} 个
          </span>
        </div>

        <div v-if="running" class="scanbar" style="margin-bottom: 8px">
          <div :style="{ width: Math.max(pct, 2) + '%' }"></div>
        </div>
        <div v-if="running" class="hint" style="margin-bottom: 12px">
          {{ progress.phase }} · {{ progress.done }}/{{ progress.total }} · {{ progress.current }}
        </div>

        <div v-if="plan?.notes?.length" style="margin-bottom: 12px">
          <div v-for="(n, i) in plan.notes" :key="i" class="hint">· {{ n }}</div>
        </div>

        <table v-if="plan" class="diff-table">
          <thead>
            <tr>
              <th>原文件</th>
              <th>导出为</th>
              <th>大小</th>
              <th>动作</th>
              <th>说明</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in plan.rows" :key="row.id" :class="{ nochange: row.action === 'skip' }">
              <td :title="row.src">{{ row.filename.slice(0, 30) }}</td>
              <td :title="row.dest">{{ relDest(row).slice(0, 46) }}</td>
              <td>{{ fmtBytes(row.sizeBytes) }}</td>
              <td :class="{ new: row.action === 'copy' }">{{ row.action === "copy" ? "复制" : "跳过" }}</td>
              <td class="note">{{ row.reason ?? "" }}</td>
            </tr>
          </tbody>
        </table>
        <div v-if="plan && plan.rows.length === 0" class="hint">这个范围里没有可导出的文件。</div>
      </div>
    </div>
  </div>
</template>
