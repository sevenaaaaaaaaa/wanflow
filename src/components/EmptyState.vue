<script setup lang="ts">
import { ref } from "vue";
import Icon from "./Icon.vue";

const emit = defineEmits<{ pick: [kind: "folder" | "photos"] }>();

// 版本号来自 package.json（vite define），发版不再需要手改界面
const version = __APP_VERSION__;

const budget = ref(4);
const shot = ref(false);

function choose(n: number) {
  budget.value = n;
  shot.value = false;
}
function reset() {
  shot.value = false;
}
</script>

<template>
  <div class="empty">
    <header class="empty-head">
      <span class="brand">Wan<em>Flow</em></span>
      <span class="ver">v{{ version }} · 本地优先</span>
    </header>

    <div class="hero">
      <div class="copy">
        <span class="eyebrow"><i></i> 本地优先 · 原图一个字节都不动</span>
        <h1>先挑一页，<br /><em>再谈几千张。</em></h1>
        <p class="lede">
          WanFlow 不让你一张张判。它把照片按天排成一页，<br />
          你只决定 <b>这一页留几张</b>，剩下的交给它。
        </p>
        <div class="doors">
          <button class="door primary" @click="emit('pick', 'folder')">
            <Icon name="folder" :size="17" /> 选择照片文件夹
            <kbd>⌘I</kbd>
          </button>
          <button class="door" @click="emit('pick', 'photos')">
            <Icon name="image" :size="17" /> 从照片图库导入
          </button>
        </div>
        <p class="fine">
          支持 QQ / 微信保存的照片 · 自动去重、从文件名恢复拍摄时间 · 随时 ⌘Z 撤销
        </p>
      </div>

      <div class="demo" aria-label="交互示例">
        <div class="demo-tag"><span></span> 示例 · 先玩一页</div>
        <div class="paper">
          <div class="grid" :data-n="budget">
            <span v-for="n in budget" :key="n" :class="['ph', 'p' + (n % 4)]"></span>
          </div>
          <div class="paper-foot">
            <span>11 月 5 日 · 周四</span>
            <span>第 1 页 · {{ budget }} 张</span>
          </div>
          <div v-if="shot" class="shot">
            <div class="stamp">
              <span class="ok"><Icon name="check" :size="15" /></span>
              <b>收进精选册</b>
              <span>这一页 · {{ budget }} 张</span>
              <button @click.stop="reset">再玩一次</button>
            </div>
          </div>
        </div>
        <div class="ruler">
          <span class="q">这一页，留几张？</span>
          <div class="ticks">
            <button
              v-for="n in 6"
              :key="n"
              :class="{ on: budget === n }"
              @click="choose(n)"
            >{{ n }}</button>
          </div>
          <button class="develop" @click="shot = true">显影 <kbd>↵</kbd></button>
        </div>
        <p class="demo-hint">点数字改张数，再按「显影」— 一天就这么过去了。</p>
      </div>
    </div>

    <ol class="steps">
      <li><b>01</b><span><strong>导入</strong>文件夹 / 照片图库，只建索引，不搬文件</span></li>
      <li><b>02</b><span><strong>一天一页</strong>自动挑出这一天的候选，先看整体</span></li>
      <li><b>03</b><span><strong>显影</strong>留下的进精选册，其余转待定，可整组撤销</span></li>
    </ol>
  </div>
</template>

<style scoped>
.empty {
  flex: 1;
  display: grid;
  align-content: center;
  gap: 34px;
  padding: 34px 40px 40px;
  max-width: 1160px;
  width: 100%;
  margin: 0 auto;
  text-align: left;
}
.hero {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(340px, 420px);
  gap: 56px;
  align-items: center;
}
.empty-head {
  position: fixed;
  top: 18px;
  left: 26px;
  display: flex;
  align-items: center;
  gap: 10px;
  z-index: 5;
}
.empty-head .brand { font-size: var(--fs-3); }
.empty-head .ver {
  font-family: var(--font-mono);
  font-size: var(--fs-1);
  letter-spacing: .04em;
  color: var(--text-4);
}
.copy { min-width: 0; }
.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: var(--fs-1);
  letter-spacing: .08em;
  text-transform: uppercase;
  color: var(--text-3);
}
.eyebrow i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
}
h1 {
  margin: 16px 0 14px;
  font-size: clamp(34px, 4.2vw, 52px);
  line-height: 1.12;
  letter-spacing: -.03em;
  font-weight: 500;
  color: var(--text-1);
}
h1 em {
  font-style: normal;
  font-family: "Songti SC", "Noto Serif SC", serif;
  font-weight: 500;
  color: var(--accent-ink);
}
.lede {
  margin: 0 0 24px;
  max-width: 30em;
  font-size: var(--fs-3);
  line-height: 1.75;
  color: var(--text-3);
}
.lede b { color: var(--text-1); font-weight: 500; }
.doors { display: flex; flex-wrap: wrap; gap: 10px; }
.door {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  height: 44px;
  padding: 0 16px;
  border-radius: 10px;
  border: 0.5px solid var(--border-2);
  background: var(--surface-2);
  color: var(--text-1);
  font-size: var(--fs-3);
  font-weight: 500;
  cursor: pointer;
  transition: background var(--dur-2) var(--ease-out), border-color var(--dur-2) var(--ease-out), transform var(--dur-2) var(--ease-out);
}
.door:hover { background: var(--surface-3); border-color: var(--border-2); transform: translateY(-1px); }
.door.primary {
  background: var(--accent);
  border-color: transparent;
  color: var(--on-accent);
}
.door.primary:hover { background: var(--accent-strong); }
.door kbd, .develop kbd {
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1;
  padding: 3px 5px;
  border-radius: 4px;
  background: oklch(0% 0 0 / .16);
  color: inherit;
  opacity: .72;
}
.door.primary kbd, .develop kbd {
  background: oklch(100% 0 0 / .34);
  opacity: 1;
}
.fine {
  margin: 16px 0 0;
  font-size: var(--fs-1);
  line-height: 1.7;
  color: var(--text-4);
}
.demo {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 14px;
  justify-self: end;
  width: 100%;
  max-width: 400px;
}
.demo-tag {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  align-self: flex-start;
  font-size: var(--fs-1);
  color: var(--text-4);
  letter-spacing: .04em;
}
.demo-tag span {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--accent);
}
.paper {
  position: relative;
  padding: 11px;
  border-radius: 3px;
  background: oklch(97% .012 95);
  color: oklch(30% .012 85);
  box-shadow: 0 0 0 .5px oklch(30% .02 95 / .18);
}
.grid {
  display: grid;
  gap: 4px;
  aspect-ratio: 4 / 3;
  width: 100%;
}
.grid[data-n="1"] { grid-template-columns: 1fr; }
.grid[data-n="2"] { grid-template-columns: 1fr 1fr; }
.grid[data-n="3"] { grid-template-columns: 1.45fr 1fr; grid-template-rows: 1fr 1fr; }
.grid[data-n="3"] .ph:first-child { grid-row: span 2; }
.grid[data-n="4"] { grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; }
.grid[data-n="5"] { grid-template-columns: 1fr 1fr 1fr; grid-template-rows: 1fr 1fr; }
.grid[data-n="5"] .ph:first-child { grid-column: span 2; }
.grid[data-n="6"] { grid-template-columns: 1fr 1fr 1fr; grid-template-rows: 1fr 1fr; }
.ph {
  position: relative;
  border-radius: 1px;
  min-height: 0;
  overflow: hidden;
  background: #6f7a76;
}
.ph::after {
  content: "";
  position: absolute;
  inset: 0;
  background: radial-gradient(120% 100% at 50% 40%, transparent 55%, oklch(0% 0 0 / .18));
}
.p0 {
  background:
    radial-gradient(46% 38% at 30% 26%, oklch(100% 0 0 / .3), transparent 72%),
    linear-gradient(165deg, #d8c3a8, #7f8b8c 58%, #37443f);
}
.p1 {
  background:
    radial-gradient(46% 38% at 68% 24%, oklch(100% 0 0 / .24), transparent 72%),
    linear-gradient(165deg, #b9c4a6, #6c8567 60%, #31422f);
}
.p2 {
  background:
    radial-gradient(46% 38% at 34% 30%, oklch(100% 0 0 / .32), transparent 72%),
    linear-gradient(165deg, #c6d4dc, #8198a1 58%, #33474d);
}
.p3 {
  background:
    radial-gradient(46% 38% at 62% 28%, oklch(100% 0 0 / .26), transparent 72%),
    linear-gradient(165deg, #e7d6bf, #b18d6c 58%, #4b3a2e);
}
.paper-foot {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 2px 2px;
  font-size: 11px;
  letter-spacing: .02em;
  color: oklch(48% .012 85);
}
.paper-foot span:first-child { font-family: "Songti SC", "Noto Serif SC", serif; }
.shot {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  background: oklch(97% .012 95 / .94);
  animation: shotIn 220ms var(--ease-out) both;
}
.stamp {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 16px 28px 15px;
  border-radius: 4px;
  background: oklch(98.5% .008 95 / .97);
  box-shadow: 0 0 0 .5px oklch(30% .02 95 / .18);
}
.stamp .ok {
  width: 28px;
  height: 28px;
  margin-bottom: 4px;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: var(--accent-soft, rgb(8 145 178 / .14));
  color: var(--accent-ink, #0e7490);
}
.shot b { font-size: 17px; font-weight: 500; letter-spacing: -.01em; color: oklch(30% .012 85); }
.shot .stamp span { font-size: 11px; color: oklch(46% .012 85); }
.shot button {
  margin-top: 9px;
  height: 26px;
  padding: 0 12px;
  border-radius: 7px;
  border: 0.5px solid oklch(30% .02 95 / .18);
  background: oklch(98.5% .008 95 / .9);
  color: oklch(30% .012 85);
  font-size: 11px;
  cursor: pointer;
}
.shot button:hover { background: oklch(30% .02 95 / .06); }
@keyframes shotIn { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }
.ruler {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.q { font-size: var(--fs-2); color: var(--text-2); }
.ticks { display: flex; align-items: center; gap: 2px; }
.ticks button {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--text-4);
  font-family: var(--font-mono);
  font-size: 11px;
  cursor: pointer;
}
.ticks button:hover { background: var(--hover); color: var(--text-1); }
.ticks button.on {
  background: var(--accent);
  color: var(--on-accent);
}
.develop {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 30px;
  padding: 0 12px;
  margin-left: auto;
  border: none;
  border-radius: 8px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: var(--fs-2);
  font-weight: 500;
  cursor: pointer;
}
.develop:hover { background: var(--accent-strong); }
.demo-hint { margin: 0; font-size: var(--fs-1); color: var(--text-4); }
.steps {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 26px;
  margin: 0;
  padding: 20px 0 0;
  list-style: none;
  border-top: 0.5px solid var(--border-1);
}
.steps li { display: flex; gap: 12px; align-items: flex-start; }
.steps b {
  font-family: var(--font-mono);
  font-size: 11px;
  letter-spacing: .06em;
  color: var(--accent);
  padding-top: 2px;
}
.steps span { font-size: var(--fs-2); line-height: 1.6; color: var(--text-3); }
.steps strong { display: block; color: var(--text-1); font-weight: 500; margin-bottom: 2px; }
@media (max-width: 1000px) {
  .hero { grid-template-columns: 1fr; gap: 34px; }
  .demo { justify-self: start; max-width: 420px; }
  .steps { grid-template-columns: 1fr; gap: 12px; }
}
@media (max-width: 680px) {
  .empty { padding: 26px 20px 34px; gap: 26px; }
  .demo { display: none; }
}
</style>
