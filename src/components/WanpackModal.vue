<script setup lang="ts">
import { computed, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  api,
  fmtBytes,
  fmtTime,
  type WanpackManifest,
  type WanpackPlan,
  type WanpackRemap,
  type WanpackRestoreOutcome,
  type WanpackStats,
} from "../api";

const props = defineProps<{ mode: "pack" | "restore" }>();
const emit = defineEmits<{ close: []; done: [msg: string] }>();

const busy = ref(false);
const error = ref("");

// 打包
const dest = ref("");
const plan = ref<WanpackPlan | null>(null);
const packed = ref<{ dest: string; bytes: number; sha: string } | null>(null);

// 恢复
const packPath = ref("");
const manifest = ref<WanpackManifest | null>(null);
const destDb = ref("");
const mapped = ref<Record<number, string>>({});
const restored = ref<WanpackRestoreOutcome | null>(null);
// 直接替换当前库：先写 pending，重启后由启动钩子交换
const replaceLive = ref(false);
const staged = ref<WanpackRestoreOutcome | null>(null);

const title = computed(() => (props.mode === "pack" ? "打包整库" : "从整库包恢复"));

const remapList = computed<WanpackRemap[]>(() =>
  Object.entries(mapped.value)
    .filter(([, to]) => !!to)
    .map(([id, to]) => {
      const src = manifest.value?.sources.find((s) => String(s.id) === id);
      return { from: src?.root ?? "", to } as WanpackRemap;
    })
    .filter((r) => !!r.from),
);

function stamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return String(d.getFullYear()) + p(d.getMonth() + 1) + p(d.getDate());
}

function statsLine(s: WanpackStats): string {
  return s.media + " 张照片 · " + s.actions + " 条动作 · " + s.tags + " 个标签 · " + s.albums + " 本精选集";
}

async function pickDest() {
  error.value = "";
  const picked = await save({
    title: "整库包保存到哪",
    defaultPath: "wanflow-" + stamp() + ".wanpack",
    filters: [{ name: "WanFlow 整库包", extensions: ["wanpack"] }],
  });
  if (!picked) return;
  dest.value = String(picked);
  plan.value = null;
  packed.value = null;
}

async function preview() {
  if (!dest.value) return;
  busy.value = true;
  error.value = "";
  try {
    plan.value = await api.wanpackPlan(dest.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function runPack() {
  if (!plan.value || plan.value.blocked.length > 0) return;
  busy.value = true;
  error.value = "";
  try {
    const out = await api.wanpackPack(dest.value, false, false);
    packed.value = { dest: out.dest, bytes: out.packBytes, sha: out.sha256 };
    emit("done", "整库包已生成：" + out.dest);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function pickPack() {
  error.value = "";
  const picked = await open({
    title: "选择 .wanpack 整库包",
    filters: [{ name: "WanFlow 整库包", extensions: ["wanpack"] }],
    multiple: false,
    directory: false,
  });
  if (!picked) return;
  packPath.value = String(picked);
  manifest.value = null;
  restored.value = null;
  mapped.value = {};
  destDb.value = packPath.value.replace(/\.wanpack$/i, "") + "-restored.db";
  busy.value = true;
  try {
    manifest.value = await api.wanpackInspect(packPath.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function pickDestDb() {
  const picked = await save({
    title: "恢复出的库存到哪（当前库不会动）",
    defaultPath: destDb.value || "wanflow-restored.db",
    filters: [{ name: "SQLite 库", extensions: ["db"] }],
  });
  if (!picked) return;
  destDb.value = String(picked);
}

async function pickRoot(id: number) {
  const picked = await open({ directory: true, title: "这些照片现在放在哪" });
  if (!picked) return;
  mapped.value = { ...mapped.value, [id]: String(picked) };
}

async function runRestore() {
  if (!manifest.value) return;
  if (!replaceLive.value && !destDb.value) return;
  busy.value = true;
  error.value = "";
  try {
    if (replaceLive.value) {
      staged.value = await api.wanpackRestoreLive(packPath.value, remapList.value, false);
    } else {
      restored.value = await api.wanpackRestore(packPath.value, destDb.value, remapList.value, false, false);
      emit("done", "整库已恢复：" + restored.value.dest);
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function restartNow() {
  busy.value = true;
  try {
    await api.restartApp();
  } catch (e) {
    error.value = String(e);
    busy.value = false;
  }
}
</script>

<template>
  <div class="modal-mask" @mousedown.self="!busy && $emit('close')">
    <div class="modal wp-modal">
      <header>
        {{ title }}
        <span class="spacer"></span>
        <button :disabled="busy" @click="$emit('close')">✕</button>
      </header>
      <div class="body">
        <template v-if="mode === 'pack'">
          <p class="wp-note">
            把整个库打成一个 .wanpack 文件：标签、精选集、时间修正、撤销历史都在里面。
            默认不搬照片，换台机器后用「从整库包恢复」接着整理。
          </p>
          <div class="form-row">
            <label>保存到</label>
            <button @click="pickDest">选择位置…</button>
            <span class="wp-path">{{ dest || "还没选" }}</span>
          </div>
          <div class="form-row">
            <button :disabled="!dest || busy" @click="preview">预览</button>
            <button class="primary" :disabled="!plan || plan.blocked.length > 0 || busy" @click="runPack">
              {{ busy ? "打包中…" : "开始打包" }}
            </button>
            <span v-if="plan" class="hint">{{ statsLine(plan.stats) }} · 库文件 {{ fmtBytes(plan.dbBytes) }}</span>
          </div>
          <div v-for="(b, i) in plan ? plan.blocked : []" :key="i" class="wp-warn">{{ b }}</div>
          <div v-if="packed" class="wp-ok">
            <b>已生成</b> {{ packed.dest }} · {{ fmtBytes(packed.bytes) }} · sha256 {{ packed.sha.slice(0, 12) }}…
          </div>
        </template>

        <template v-else>
          <p class="wp-note">
            选一个 .wanpack，恢复成新的库文件。默认只产出副本、不动当前库；
            勾选「直接替换当前库」则写入待替换文件，下次启动时接管，旧库自动备份。
          </p>
          <div class="form-row">
            <label>整库包</label>
            <button @click="pickPack">选择…</button>
            <span class="wp-path">{{ packPath || "还没选" }}</span>
          </div>
          <div v-if="manifest" class="wp-facts">
            <div><b>来自</b> WanFlow v{{ manifest.appVersion }} · 打包于 {{ fmtTime(manifest.createdAt) }}</div>
            <div><b>内容</b> {{ statsLine(manifest.stats) }} · 库文件 {{ fmtBytes(manifest.files[0] ? manifest.files[0].bytes : 0) }}</div>
          </div>
          <div class="form-row">
            <label>方式</label>
            <label class="wp-check">
              <input type="checkbox" v-model="replaceLive" />
              直接替换当前库（重启后生效）
            </label>
          </div>
          <div v-if="replaceLive" class="wp-facts">
            <div>旧库先备份成 wanflow.db.bak-&lt;时间&gt;，新库在下次启动时接管；准备好后需要重启应用。</div>
          </div>
          <div v-else class="form-row">
            <label>恢复到</label>
            <button :disabled="!manifest" @click="pickDestDb">选择位置…</button>
            <span class="wp-path">{{ destDb || "还没选" }}</span>
          </div>

          <div v-if="manifest && manifest.sources.length" class="wp-sources">
            <div class="wp-sub">照片目录映射：不选的保留原路径，恢复后这些照片会显示为找不到</div>
            <div v-for="s in manifest.sources" :key="s.id" class="wp-src">
              <span class="wp-src-label">{{ s.label }}</span>
              <span class="wp-src-path">{{ s.root || "（网盘来源）" }}</span>
              <button v-if="s.root" @click="pickRoot(s.id)">{{ mapped[s.id] ? "改…" : "选择文件夹…" }}</button>
              <span v-if="mapped[s.id]" class="wp-src-to">→ {{ mapped[s.id] }}</span>
              <span v-else-if="s.root" class="hint">保持原路径</span>
            </div>
          </div>

          <div class="form-row">
            <button class="primary" :disabled="!manifest || (!replaceLive && !destDb) || busy" @click="runRestore">
              {{ busy ? "恢复中…" : replaceLive ? "准备好替换" : "开始恢复" }}
            </button>
            <span v-if="manifest && !replaceLive" class="hint">恢复完当前库仍然不变</span>
          </div>
          <div v-if="staged" class="wp-ok">
            <b>已准备好</b> 重启后由新库接管 · 重写 {{ staged.mediaRewritten }} 张路径 · 未映射 {{ staged.mediaUnmapped }} 张
            <button class="wp-restart" :disabled="busy" @click="restartNow">立即重启</button>
          </div>
          <div v-if="restored" class="wp-ok">
            <b>已恢复</b> {{ restored.dest }} · 重写 {{ restored.mediaRewritten }} 张路径 · 未映射 {{ restored.mediaUnmapped }} 张
          </div>
        </template>

        <p v-if="error" class="wp-err">{{ error }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 内容比导出面板短，收窄到 640，避免右侧大片留白 */
.wp-modal {
  width: min(640px, 92vw);
  max-height: 80vh;
}
.wp-note {
  margin-bottom: var(--sp-16);
  max-width: 640px;
  color: var(--text-3);
  font-size: var(--fs-2);
  line-height: 1.7;
}
.wp-path {
  max-width: 420px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-3);
  font-family: var(--font-mono);
  font-size: var(--fs-1);
}
.wp-facts {
  display: grid;
  gap: var(--sp-1);
  margin-bottom: var(--sp-12);
  padding: var(--sp-12);
  border: 0.5px solid var(--border-1);
  border-radius: var(--r-md);
  background: var(--hover);
  color: var(--text-2);
  font-size: var(--fs-2);
}
.wp-facts b {
  margin-right: var(--sp-2);
  color: var(--text-4);
  font-weight: 500;
}
.wp-sources {
  margin-bottom: var(--sp-16);
}
.wp-sub {
  margin-bottom: var(--sp-2);
  color: var(--text-4);
  font-size: var(--fs-1);
}
.wp-src {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2) 0;
  border-bottom: 0.5px solid var(--border-1);
  font-size: var(--fs-2);
}
.wp-src-label {
  min-width: 96px;
  color: var(--text-2);
}
.wp-src-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-4);
  font-family: var(--font-mono);
  font-size: var(--fs-1);
}
.wp-src-to {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--accent-ink);
  font-size: var(--fs-1);
}
.wp-warn {
  margin-bottom: var(--sp-2);
  color: var(--warn-ink);
  font-size: var(--fs-2);
}
.wp-err {
  margin-top: var(--sp-8);
  color: var(--danger-ink);
  font-size: var(--fs-2);
}
.wp-ok {
  margin-top: var(--sp-8);
  color: var(--ok-ink);
  font-size: var(--fs-2);
}
.wp-ok b {
  font-weight: 500;
}
.wp-check {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: auto;
}
.wp-restart {
  margin-left: var(--sp-3);
}

</style>
