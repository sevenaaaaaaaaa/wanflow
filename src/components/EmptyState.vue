<script setup lang="ts">
import Icon from "./Icon.vue";
defineEmits<{ pick: [] }>();
</script>

<template>
  <div class="empty">
    <div class="mark"><Icon name="layers" :size="44" /></div>
    <h2>把家庭照片文件夹交给 WanFlow</h2>
    <p>
      分析只在这台电脑上跑，原图一个字节都不动。<br />
      自动去重、从文件名恢复拍摄时间，键盘快速挑拣，<br />
      最后导出精选集，直接去做相册和成长视频。
    </p>
    <button class="primary cta" style="height: 38px; padding: 0 22px" @click="$emit('pick')">
      <Icon name="folder" :size="15" /> 选择照片文件夹
    </button>
    <p style="font-size: var(--fs-1); color: var(--text-4)">
      支持 QQ 空间 / 微信保存的照片 · 自动从文件名恢复拍摄时间
    </p>
  </div>
</template>
