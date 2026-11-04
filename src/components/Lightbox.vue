<script setup lang="ts">
import { computed } from "vue";
import { assetUrl, fmtBytes, fmtTime, previewUrl, type MediaItem } from "../api";
import Icon from "./Icon.vue";

const props = defineProps<{ item: MediaItem; pos: number; count: number }>();
defineEmits<{
  close: [];
  prev: [];
  next: [];
  decide: [d: "keep" | "reject" | "pending"];
  stars: [s: number];
  dup: [g: number];
  arena: [g: number];
}>();

const src = computed(() => (props.item.kind === "video" ? assetUrl(props.item.path) : previewUrl(props.item)));
</script>

<template>
  <div class="lightbox" @mousedown.self="$emit('close')">
    <div class="lb-main">
      <button class="lb-nav prev" @click="$emit('prev')" :disabled="pos <= 1" title="上一张 ←">‹</button>
      <video v-if="item.kind === 'video'" :src="src" controls autoplay />
      <img v-else :src="src" @click="$emit('next')" title="点击下一张" draggable="false" />
      <button class="lb-nav next" @click="$emit('next')" :disabled="pos >= count" title="下一张 →">›</button>
    </div>

    <div class="lb-bar">
      <span class="lb-meta">
        <b>{{ pos }}/{{ count }}</b> · {{ item.filename }} · {{ fmtTime(item.effTime) }}（{{ item.timeSource }}）
        · {{ fmtBytes(item.sizeBytes) }}
        <template v-if="item.width"> · {{ item.width }}×{{ item.height }}</template>
        <template v-if="item.quality !== null"> · 质量 {{ Math.round(item.quality) }}</template>
        <span v-if="item.dupGroup !== null" class="chip accent" style="cursor: pointer; margin-left: 8px" @click="$emit('dup', item.dupGroup!)">重复组</span>
        <span v-if="item.burstGroup !== null" class="chip" style="cursor: pointer; margin-left: 6px" @click="$emit('arena', item.burstGroup!)">连拍擂台</span>
      </span>

      <span class="starpick" aria-label="星级">
        <button
          v-for="s in 5"
          :key="s"
          class="starbtn"
          :class="{ on: (item.stars ?? 0) >= s }"
          :title="`${s} 星（${s}）`"
          @click="$emit('stars', (item.stars === s ? 0 : s))"
        >★</button>
      </span>
      <span style="width: 1px; height: 18px; background: var(--border-2)"></span>
      <button class="primary" @click="$emit('decide', 'keep')"><Icon name="check" :size="13" /> 保留</button>
      <button @click="$emit('decide', 'pending')">待定</button>
      <button class="danger" @click="$emit('decide', 'reject')"><Icon name="x" :size="13" /> 淘汰</button>
      <button class="ghost" @click="$emit('close')"><kbd>esc</kbd></button>
    </div>
  </div>
</template>