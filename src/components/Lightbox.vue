<script setup lang="ts">
import { computed } from "vue";
import { assetUrl, fmtBytes, fmtTime, previewUrl, type MediaItem } from "../api";

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
  <div class="lightbox" @click.self="$emit('close')">
    <div class="lb-main">
      <button class="lb-nav prev" @click="$emit('prev')" :disabled="pos <= 1">‹</button>
      <video v-if="item.kind === 'video'" :src="src" controls autoplay />
      <img v-else :src="src" @click="$emit('next')" title="点击下一张" />
      <button class="lb-nav next" @click="$emit('next')" :disabled="pos >= count">›</button>
    </div>
    <div class="lb-bar">
      <span class="lb-meta">
        {{ pos }}/{{ count }} · {{ item.filename }} · {{ fmtTime(item.effTime) }}（{{ item.timeSource }}） ·
        {{ fmtBytes(item.sizeBytes) }}
        <template v-if="item.width"> · {{ item.width }}×{{ item.height }}</template>
        <template v-if="item.quality !== null"> · 质量 {{ Math.round(item.quality) }}（清晰 {{ Math.round(item.sharpness ?? 0) }} / 曝光 {{ Math.round(item.exposure ?? 0) }}）</template>
        <span
          v-if="item.dupGroup !== null"
          class="chip"
          style="cursor: pointer; margin-left: 6px"
          @click="$emit('dup', item.dupGroup!)"
          >查看重复组</span
        >
        <span
          v-if="item.burstGroup !== null"
          class="chip"
          style="cursor: pointer; margin-left: 6px"
          @click="$emit('arena', item.burstGroup!)"
        >
          连拍擂台
        </span>
      </span>
      <button
        v-for="s in 5"
        :key="s"
        class="starbtn"
        :class="{ on: (item.stars ?? 0) >= s }"
        @click="$emit('stars', (item.stars === s ? 0 : s))"
      >
        ★
      </button>
      <span style="width: 1px; height: 22px; background: var(--line)"></span>
      <button class="primary" @click="$emit('decide', 'keep')">保留 P</button>
      <button @click="$emit('decide', 'pending')">待定 U</button>
      <button style="color: #fca5a5" @click="$emit('decide', 'reject')">淘汰 X</button>
      <button @click="$emit('close')">关闭 Esc</button>
    </div>
  </div>
</template>
