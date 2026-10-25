<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api, fmtTime, type SourceItem } from "../api";

const emit = defineEmits<{ close: []; started: [] }>();

const tab = ref<"folder" | "photos" | "webdav" | "qq" | "manage">("folder");
const sources = ref<SourceItem[]>([]);
const error = ref("");
const busy = ref(false);

// WebDAV 表单
const davUrl = ref("");
const davUser = ref("");
const davPass = ref("");
const davPath = ref("");
const savingPass = ref(true);

const KIND_LABEL: Record<string, string> = {
  folder: "📁 本地文件夹",
  photos: "🖼️ 照片图库",
  webdav: "☁️ 网盘",
  qq: "🐧 QQ·微信",
};

async function refreshSources() {
  sources.value = await api.listSources();
}
onMounted(refreshSources);

function fail(e: unknown) {
  error.value = String(e);
  busy.value = false;
}

async function pickFolder(kind: "folder" | "qq") {
  error.value = "";
  const dirPath = await open({
    directory: true,
    title: kind === "qq" ? "选择 QQ/微信导出目录" : "选择照片文件夹",
  });
  if (!dirPath) return;
  const name = String(dirPath).split("/").filter(Boolean).pop() ?? String(dirPath);
  try {
    busy.value = true;
    await api.importFolder(String(dirPath), kind, name);
    emit("started");
  } catch (e) {
    fail(e);
  }
}

async function importPhotos() {
  error.value = "";
  try {
    busy.value = true;
    await api.importPhotosLibrary();
    emit("started");
  } catch (e) {
    fail(e);
  }
}

async function importDav() {
  error.value = "";
  if (!davUrl.value || !davUser.value) {
    error.value = "请填写地址和用户名";
    return;
  }
  try {
    busy.value = true;
    await api.importWebdav({
      url: davUrl.value,
      username: davUser.value,
      password: davPass.value,
      remotePath: davPath.value,
    });
    emit("started");
  } catch (e) {
    fail(e);
  }
}

async function rescan(id: number) {
  error.value = "";
  try {
    busy.value = true;
    await api.rescanSource(id);
    emit("started");
  } catch (e) {
    fail(e);
  }
}

async function unlink(id: number) {
  await api.unlinkSource(id);
  await refreshSources();
}

defineExpose({ refreshSources, fail });
</script>

<template>
  <div class="modal-mask" @click.self="$emit('close')">
    <div class="modal">
      <header>
        导入照片 / 视频
        <span class="spacer"></span>
        <button @click="$emit('close')">✕</button>
      </header>
      <div class="body">
        <div class="import-tabs">
          <button :class="{ on: tab === 'folder' }" @click="tab = 'folder'">📁 本地文件夹</button>
          <button :class="{ on: tab === 'photos' }" @click="tab = 'photos'">🖼️ 照片图库</button>
          <button :class="{ on: tab === 'webdav' }" @click="tab = 'webdav'">☁️ 网盘 WebDAV</button>
          <button :class="{ on: tab === 'qq' }" @click="tab = 'qq'">🐧 QQ·微信</button>
          <button :class="{ on: tab === 'manage' }" @click="tab = 'manage'">⚙️ 来源 ({{ sources.length }})</button>
        </div>

        <p v-if="error" class="import-error">{{ error }}</p>

        <div v-if="tab === 'folder'" class="import-pane">
          <p class="import-desc">
            选择电脑上的任意文件夹，WanFlow 会建立索引而<b>不移动、不修改</b>任何原文件。<br />
            自动去重、从文件名和 EXIF 恢复拍摄时间。
          </p>
          <button class="primary" :disabled="busy" @click="pickFolder('folder')">选择文件夹并导入</button>
        </div>

        <div v-else-if="tab === 'photos'" class="import-pane">
          <p class="import-desc">
            直接读取 macOS 照片图库中的<b>原片</b>（引用式，不复制不修改）。<br />
            iCloud 未下载到本地的照片会自动跳过，之后可重新导入。
          </p>
          <button class="primary" :disabled="busy" @click="importPhotos">导入照片图库</button>
          <p class="hint" style="margin-top: 10px">
            安装过 osxphotos 则能保留原始文件名；未安装也能导入（用 EXIF 时间，无需任何依赖）。
          </p>
        </div>

        <div v-else-if="tab === 'webdav'" class="import-pane">
          <p class="import-desc">
            支持 Alist / 群晖 / 坚果云等任何 WebDAV 服务。<br />
            用 <a href="https://alistgo.com" target="_blank">Alist</a> 可把百度网盘、阿里云盘、夸克一键变成 WebDAV。
          </p>
          <div class="form-row"><label>地址</label><input v-model="davUrl" placeholder="https://dav.example.com/dav/" style="flex: 1" /></div>
          <div class="form-row"><label>用户名</label><input v-model="davUser" style="flex: 1" /></div>
          <div class="form-row"><label>密码</label><input v-model="davPass" type="password" placeholder="应用密码/授权码" style="flex: 1" /></div>
          <div class="form-row"><label>远程目录</label><input v-model="davPath" placeholder="留空 = 根目录，如 photos/2023/" style="flex: 1" /></div>
          <p class="hint">密码只存进系统钥匙串，WanFlow 不上传、不落盘。导入的文件复制到本地暂存区并保留原始拍摄时间。</p>
          <button class="primary" :disabled="busy" style="margin-top: 8px" @click="importDav">连接并导入</button>
        </div>

        <div v-else-if="tab === 'qq'" class="import-pane">
          <p class="import-desc">
            导入用 <b>企鹅相册（QzonePhoto）</b>、QQ 空间导出助手等工具下载到本地的 QQ 空间照片。<br />
            这些照片普遍丢失了 EXIF，WanFlow 会从文件名时间戳自动恢复拍摄时间。
          </p>
          <button class="primary" :disabled="busy" @click="pickFolder('qq')">选择导出目录并导入</button>
        </div>

        <div v-else class="import-pane">
          <p v-if="sources.length === 0" class="hint">还没有导入过任何来源</p>
          <div v-for="s in sources" :key="s.id" class="source-row">
            <span>{{ KIND_LABEL[s.kind] ?? s.kind }}</span>
            <b :title="s.rootPath ?? s.url ?? ''">{{ s.label }}</b>
            <span class="hint">{{ s.mediaCount }} 张</span>
            <span class="hint">{{ s.lastScanned ? "上次同步 " + fmtTime(s.lastScanned) : "未同步" }}</span>
            <span class="spacer"></span>
            <button style="padding: 4px 10px" :disabled="busy" @click="rescan(s.id)">重新同步</button>
            <button style="padding: 4px 10px" title="文件与决策保留，仅解除关联" @click="unlink(s.id)">解除</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.import-tabs { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 14px; }
.import-tabs button { border-radius: 8px; }
.import-tabs button.on { background: var(--accent); border-color: var(--accent); color: #fff; }
.import-desc { color: var(--muted); line-height: 1.8; margin-bottom: 14px; font-size: 13px; }
.import-error { color: #fca5a5; margin-bottom: 12px; }
.source-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 6px;
  border-bottom: 1px solid var(--line);
  font-size: 13px;
}
</style>
