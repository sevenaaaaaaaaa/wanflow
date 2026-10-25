<script setup lang="ts">
import { computed, ref } from "vue";
import { api, fmtTime, type BatchTimeSpec, type TimeDiffRow } from "../api";

const props = defineProps<{ ids: number[] }>();
const emit = defineEmits<{ close: []; applied: [label: string] }>();

const mode = ref<"offset" | "reparse" | "anchor">("offset");
const hours = ref(-8);
const minutes = ref(0);
// 锚点模式：以第一张选中照片为锚
const anchorNewTime = ref<string>("");
const diff = ref<TimeDiffRow[] | null>(null);
const previewing = ref(false);
const applying = ref(false);
const error = ref("");

const spec = computed<BatchTimeSpec | null>(() => {
  if (props.ids.length === 0) return null;
  if (mode.value === "offset") {
    return { ids: props.ids, mode: "offset", hours: hours.value, minutes: minutes.value };
  }
  if (mode.value === "reparse") {
    return { ids: props.ids, mode: "reparse" };
  }
  // anchor：锚 = 选中集合中的第一张（按 id），目标时间由用户填
  if (!anchorNewTime.value) return null;
  const anchorMedia = Math.min(...props.ids);
  const [date, time] = anchorNewTime.value.split("T");
  const [y, mo, d] = date.split("-").map(Number);
  const [h, mi] = time.split(":").map(Number);
  const ts = Date.UTC(y, mo - 1, d, h, mi) / 1000;
  return { ids: props.ids, mode: "anchor", anchorMedia, anchorNewTime: ts };
});

const changedCount = computed(() => diff.value?.filter(d => d.changed).length ?? 0);

async function preview() {
  if (!spec.value) return;
  previewing.value = true;
  error.value = "";
  try {
    diff.value = await api.batchTimePreview(spec.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    previewing.value = false;
  }
}

async function apply() {
  if (!spec.value || changedCount.value === 0) return;
  applying.value = true;
  error.value = "";
  try {
    const label = await api.batchTimeApply(spec.value);
    emit("applied", label);
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    applying.value = false;
  }
}
</script>

<template>
  <div class="modal-mask" @click.self="$emit('close')">
    <div class="modal">
      <header>
        批量修正拍摄时间（{{ ids.length }} 张 · 原图不会被修改，随时可撤销）
        <span class="spacer"></span>
        <button @click="$emit('close')">✕</button>
      </header>
      <div class="body">
        <div class="form-row">
          <label>方式</label>
          <label><input type="radio" value="offset" v-model="mode" /> 整体偏移</label>
          <label><input type="radio" value="reparse" v-model="mode" /> 从文件名重新解析</label>
          <label><input type="radio" value="anchor" v-model="mode" /> 锚点对齐</label>
        </div>

        <div class="form-row" v-if="mode === 'offset'">
          <label>偏移量</label>
          <input type="number" v-model.number="hours" style="width: 90px" /> 小时
          <input type="number" v-model.number="minutes" style="width: 90px" /> 分钟
          <span class="hint">负数提前。例：相机时区没调 → 填 -8</span>
        </div>

        <div class="form-row" v-if="mode === 'reparse'">
          <span class="hint">
            从文件名中的时间戳（IMG_20231005_123456 / mmexport1696… / Screenshot_…）重新恢复拍摄时间，
            对 QQ/微信保存后 EXIF 丢失的照片最有效。
          </span>
        </div>

        <div class="form-row" v-if="mode === 'anchor'">
          <label>锚点真实时间</label>
          <input type="datetime-local" v-model="anchorNewTime" />
          <span class="hint">
            选中照片中编号最小的一张作为锚点：你只需记得它实际是什么时候拍的，
            其余照片按同样偏移整体对齐。
          </span>
        </div>

        <p v-if="error" style="color: #fca5a5; margin-bottom: 12px">{{ error }}</p>

        <div class="form-row">
          <button :disabled="!spec || previewing" @click="preview">
            {{ previewing ? "计算中…" : "预览变更 (dry-run)" }}
          </button>
          <button class="primary" :disabled="!diff || changedCount === 0 || applying" @click="apply">
            {{ applying ? "应用中…" : `应用 ${changedCount} 处变更` }}
          </button>
          <span v-if="diff" class="hint">
            {{ changedCount }} 张会变化 / {{ diff.length }} 张选中
          </span>
        </div>

        <table v-if="diff" class="diff-table">
          <thead>
            <tr>
              <th>文件</th>
              <th>当前时间</th>
              <th>来源</th>
              <th>新时间</th>
              <th>说明</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="d in diff" :key="d.id" :class="{ nochange: !d.changed }">
              <td :title="d.filename">{{ d.filename.slice(0, 38) }}</td>
              <td>{{ fmtTime(d.oldTime) }}</td>
              <td class="note">{{ d.oldSource }}</td>
              <td :class="{ new: d.changed }">{{ d.newTime ? fmtTime(d.newTime) : "—" }}</td>
              <td class="note">{{ d.note ?? (d.changed ? "" : "无变化") }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
