<script setup lang="ts">
defineEmits<{ pick: [] }>();
</script>

<template>
  <div class="empty">
    <div class="logo">🗂️</div>
    <h2>把家庭照片文件夹交给 WanFlow</h2>
    <p>
      导入后一切分析只在你的电脑上进行，原图永不被修改。<br />
      自动去重、从文件名恢复拍摄时间、键盘快速挑拣，<br />
      最后导出精选集去做相册和成长视频。
    </p>
    <button class="primary" style="font-size: 15px; padding: 10px 28px" @click="$emit('pick')">
      选择照片文件夹
    </button>
    <p style="font-size: 12px; opacity: 0.7">
      支持从 QQ 空间 / 微信保存的照片（自动从文件名恢复拍摄时间）
    </p>
  </div>
</template>
