<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import Icon from "./Icon.vue";
import { api, thumbUrl, type ClusterItem, type FamilyIndexSummary, type FamilyMember } from "../api";

const emit = defineEmits<{ close: []; refresh: [] }>();

const ready = ref(false);
const busy = ref(false);
const progressMsg = ref("");
const error = ref("");
const clusters = ref<ClusterItem[]>([]);
const members = ref<FamilyMember[]>([]);
const namingFor = ref<number | null>(null);
const newName = ref("");
const summary = ref<FamilyIndexSummary | null>(null);

async function refresh() {
  clusters.value = await api.familyClusters();
  members.value = await api.familyMembers();
}
onMounted(async () => {
  const s = await api.clipStatus();
  ready.value = (s as any).modelsReady ?? false;
  // 人脸模型状态单独判断：尝试列簇，若报模型缺失则提示下载
  await refresh();
  checkFaceModels();
});
onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});

async function checkFaceModels() {
  // 以 clusters 是否为空 + 一个轻量判定：直接尝试索引命令报错信息
  try {
    await api.familyIndexAll();
    busy.value = true;
    poll();
  } catch (e) {
    if (String(e).includes("模型还没下载")) {
      ready.value = false;
    } else if (String(e).includes("已有任务")) {
      busy.value = true;
      poll();
    }
  }
}

let timer: number | undefined;
function poll() {
  if (timer) return;
  timer = window.setInterval(async () => {
    const s = await api.getScanStatus();
    if (!s.running) {
      window.clearInterval(timer);
      timer = undefined;
      busy.value = false;
      progressMsg.value = "";
      if (s.phase && s.phase !== "完成") error.value = s.phase;
      await refresh();
      emit("refresh");
    } else {
      progressMsg.value = `${s.done}/${s.total} · ${s.current}`;
    }
  }, 500);
}

async function indexFaces() {
  error.value = "";
  try {
    busy.value = true;
    await api.familyIndexAll();
    poll();
  } catch (e) {
    fail(String(e));
  }
}

function fail(e: string) {
  error.value = e;
  busy.value = false;
  progressMsg.value = "";
}

async function register() {
  if (namingFor.value === null || !newName.value.trim()) return;
  try {
    await api.familyRegister(namingFor.value, newName.value.trim());
    namingFor.value = null;
    newName.value = "";
    await refresh();
    emit("refresh");
  } catch (e) {
    fail(String(e));
  }
}

async function assignTo(cluster: number, personId: number) {
  await api.familyAssignCluster(cluster, personId);
  await refresh();
}

async function removeMember(personId: number) {
  await api.familyRemove(personId);
  await refresh();
  emit("refresh");
}
</script>

<template>
  <div class="dup-panel" style="width: 440px">
    <header>
      <Icon name="users" :size="13" /> 家人
      <span class="spacer"></span>
      <button @click="$emit('close')">✕</button>
    </header>

    <div style="padding: 12px 14px; border-bottom: 1px solid var(--border-1)">
      <p v-if="error" class="import-error">{{ error }}</p>
      <p v-if="progressMsg" class="ai-progress">⏳ {{ progressMsg }}</p>

      <!-- 已注册家人 -->
      <div v-if="members.length > 0" style="margin-bottom: 12px">
        <p class="hint" style="margin-bottom: 6px">已注册家人（选片时会优先）</p>
        <div v-for="m in members" :key="m.id" class="member-row">
          <b>{{ m.name }}</b>
          <span class="hint">{{ m.photoCount }} 张照片</span>
          <span class="spacer"></span>
          <button style="padding: 3px 8px; color: var(--danger-ink)" @click="removeMember(m.id)">取消</button>
        </div>
      </div>

      <button v-if="!ready && clusters.length === 0" class="primary" :disabled="busy" @click="indexFaces">
        {{ busy ? "索引中…" : "下载人脸模型并扫描" }}
      </button>
      <button v-else class="primary" :disabled="busy" @click="indexFaces">
        {{ busy ? "扫描中…" : "重新扫描人脸" }}
      </button>
      <p class="hint" style="margin-top: 6px">
        检测 + 识别全在本机进行，人脸特征只存本地数据库。
      </p>
    </div>

    <div class="dup-list">
      <p class="hint" style="padding: 0 4px 8px">
        下面是照片里出现的人（按出现次数排序）。点「这是家人」给经常出现的面孔起名字。
      </p>
      <div v-for="c in clusters.slice(0, 30)" :key="c.cluster" class="dup-card">
        <img v-if="c.repThumb" :src="thumbUrl({ thumb: c.repThumb } as any)" />
        <div class="info">
          <b v-if="c.personName">{{ c.personName }} <span v-if="c.isFamily" style="color: var(--keep)">· 家人</span></b>
          <b v-else>未命名</b>
          <span>{{ c.photoCount }} 张照片出现 {{ c.faceCount }} 次</span>
        </div>
        <div style="display: flex; flex-direction: column; gap: 4px">
          <button v-if="!c.personId" class="primary" style="padding: 4px 10px" @click="namingFor = c.cluster; newName = ''">
            这是家人
          </button>
          <select
            v-else-if="members.length > 1"
            style="padding: 3px 6px; font-size: 11px"
            @change="assignTo(c.cluster, Number(($event.target as HTMLSelectElement).value))"
          >
            <option value="">归入…</option>
            <option v-for="m in members" :key="m.id" :value="m.id">{{ m.name }}</option>
          </select>
        </div>
      </div>
      <p v-if="clusters.length === 0" class="hint">还没有人脸数据——先点上面的扫描</p>
    </div>

    <!-- 命名弹层 -->
    <div v-if="namingFor !== null" class="modal-mask" @click.self="namingFor = null" style="z-index: 210">
      <div class="modal" style="width: 320px">
        <header>给这位家人起名字</header>
        <div class="body">
          <input v-model="newName" placeholder="如：糖糖 / 爸爸" style="width: 100%" @keyup.enter="register" />
          <div style="display: flex; gap: 8px; margin-top: 12px">
            <button class="primary" style="flex: 1" @click="register">保存</button>
            <button @click="namingFor = null">取消</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.member-row { display: flex; align-items: center; gap: 8px; padding: 6px 0; font-size: 13px; }
</style>
