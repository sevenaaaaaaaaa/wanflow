<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, thumbUrl, type SuggestionItem, type SuggestionRule } from "../api";

const props = defineProps<{ items: SuggestionItem[] }>();
const emit = defineEmits<{ close: []; applied: [label: string]; refresh: [] }>();

const RULE_META: Record<SuggestionRule, { title: string; desc: string; icon: string }> = {
  blur: { title: "画面模糊", desc: "清晰度明显低于正常水平", icon: "🌫️" },
  exposure: { title: "曝光失误", desc: "严重过曝或欠曝，画面看不清", icon: " expose " },
  burst: { title: "连拍多余", desc: "连拍中已自动选出最佳，其余为候选淘汰", icon: "📸" },
};

const byRule = computed(() => {
  const map = new Map<SuggestionRule, SuggestionItem[]>();
  for (const it of props.items) {
    if (!map.has(it.rule)) map.set(it.rule, []);
    map.get(it.rule)!.push(it);
  }
  return map;
});
const checked = ref<Set<number>>(new Set(props.items.map(i => i.id)));

function ruleMeta(rule: SuggestionRule) {
  return RULE_META[rule] ?? { title: rule, desc: "", icon: "❓" };
}

function toggle(id: number) {
  checked.value.has(id) ? checked.value.delete(id) : checked.value.add(id);
}
function checkRule(rule: SuggestionRule, on: boolean) {
  for (const it of byRule.value.get(rule) ?? []) {
    on ? checked.value.add(it.id) : checked.value.delete(it.id);
  }
}

async function resolve(apply: boolean) {
  const ids = [...checked.value];
  if (!ids.length) return;
  try {
    const label = await api.resolveSuggestions(ids, apply);
    emit("applied", label);
    emit("refresh");
  } catch (e) {
    emit("applied", `操作失败: ${e}`);
  }
}
</script>

<template>
  <div class="dup-panel" style="width: 430px">
    <header>
      🤖 智能初筛 · {{ items.length }} 条建议
      <span class="spacer"></span>
      <button @click="$emit('close')">✕</button>
    </header>
    <div class="dup-list">
      <p class="hint" style="padding: 0 4px 8px">
        AI 只提建议不删除：应用后才会进入"淘汰"，全部可在历史里一键撤销。
      </p>
      <div v-for="(list, rule) in byRule" :key="rule" class="rule-block">
        <div class="rule-head">
          <span>{{ ruleMeta(rule).icon }}</span>
          <b>{{ ruleMeta(rule).title }}</b>
          <span class="hint">{{ list.length }} 张 · {{ ruleMeta(rule).desc }}</span>
        </div>
        <div
          v-for="it in list"
          :key="it.id"
          class="dup-card"
          :class="{ selected: checked.has(it.id) }"
        >
          <input
            type="checkbox"
            :checked="checked.has(it.id)"
            @change="toggle(it.id)"
            style="flex-shrink: 0"
          />
          <img v-if="it.thumb" :src="thumbUrl({ thumb: it.thumb } as any)" style="cursor: pointer" @click="toggle(it.id)" />
          <div class="info">
            <b :title="it.filename">{{ it.filename }}</b>
            <span style="color: #fca5a5">{{ it.reason }}</span>
          </div>
        </div>
        <div class="rule-actions">
          <button style="padding: 4px 10px" @click="checkRule(rule, true)">全选</button>
          <button style="padding: 4px 10px" @click="checkRule(rule, false)">取消</button>
        </div>
      </div>
      <p v-if="items.length === 0" class="hint">暂无建议——先跑一次"质量分析"</p>
    </div>
    <div style="padding: 10px 14px; border-top: 1px solid var(--line); display: flex; gap: 8px; align-items: center">
      <button class="primary" :disabled="checked.size === 0" @click="resolve(true)">
        应用选中的 {{ checked.size }} 条
      </button>
      <button :disabled="checked.size === 0" @click="resolve(false)">忽略选中</button>
      <span class="spacer"></span>
      <span class="hint">忽略后不再提示</span>
    </div>
  </div>
</template>

<style scoped>
.rule-block { margin-bottom: 14px; }
.rule-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 4px;
  font-size: 13px;
}
.rule-actions { display: flex; gap: 6px; padding: 2px 4px 0; }
.dup-card .info { display: flex; flex-direction: column; gap: 3px; }
.dup-card .info span { font-size: 11.5px; }
</style>
