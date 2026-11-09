# WanFlow 设计契约

> 来源：OpenFlow 家族前台设计系统（`OpenFlowDev/docs/DESIGN-SYSTEM.md` + `assets/tokens.css` v6 终版）。
> WanFlow 是家族成员，**沿用同一套契约，不自造零件**。
> 任何页面、任何 agent 生成的界面，都以此文件为准。

---

## 一、三层结构

```
assets/tokens.css    设计契约 —— 颜色（全 oklch）、字体、圆角、阴影、缓动、亮暗模式。41 个令牌
                     （其中 30 个随主题翻转，11 个与主题无关：字体 / 圆角 / 缓动 / 容器宽）
assets/ui.css        共享组件层 —— 顶栏、按钮、卡片、徽章、玻璃窗、节标题…（跨页复用才进这里）
页面 <style>          只放这一页独有、别处不会再用的部件
```

加载顺序固定：`tokens.css → ui.css → 页面 <style>`。
页面样式在共享层之后，所以页面可以覆盖共享层，共享层永远不知道页面的存在。

**判据**：第二个页面要用同一个东西时，才把它从页面 `<style>` 移进 `ui.css`。

---

## 二、硬规则（违反即返工）

### 颜色

1. **零 hex、零 rgb、零 Tailwind 类。** 颜色只能来自 `tokens.css` 的变量。
2. 需要派生（半透明、加深、混色）一律 `color-mix(in oklab, var(--x) N%, transparent)`。
3. 新增颜色必须同时改亮暗两套 `:root` / `[data-theme="dark"]`。
4. 状态色只有三档：`--ok` `--warn` `--danger`，配 `--*-soft` 做底。不要自创第四种。

### 对比度（实测，不是估计）

| 主题 | 变量 | 色值 | 对比度 | 判定 |
|---|---|---|--:|---|
| 亮 | `--fg` | `#211910` on `#f8f3e8` | 15.67:1 | AAA |
| 亮 | `--muted` | `#5e574f` | 6.45:1 | AA |
| 亮 | `--faint` | `#6b655d` | **5.20:1** | AA 正文 |
| 亮 | `--accent` | `#1c65c8` | 5.08:1 | AA |
| 暗 | `--fg` | `#eae7e2` on `#18130d` | 15.04:1 | AAA |
| 暗 | `--muted` | `#a39e95` | 6.91:1 | AA |
| 暗 | `--faint` | `#918b83` | **5.49:1** | AA 正文 |
| 暗 | `--accent` | `#76acfc` | 7.99:1 | AA |

**`--faint` 是这套契约最容易被抄坏的地方**：暗色下若沿用亮色的 51% L，对比度会掉到 3 出头，正文级小字不合格。所以暗色单独提到 64% L。改这档之前先量。

### 字体与节奏

| 用途 | 变量 | 值 |
|---|---|---|
| 显示声部（标题、大数字） | `--font-display` | Space Grotesk + PingFang / HarmonyOS Sans SC |
| 正文 | `--font-body` | 同上，回退顺序不同 |
| 数字、代号、kicker | `--font-mono` | SF Mono / JetBrains Mono + `tabular-nums` |

- 圆角只有三档：`--r-lg` 26px（大容器）、`--r-md` 18px（卡片/窗口）、`--r-sm` 12px（按钮/输入）。
- 容器宽 `--container` 1120px，左右内边距 24px。
- 缓动：**弹簧** `--ease-spring`（cubic-bezier(.32,.72,0,1)）用于位移与缩放，`--ease-out` 用于淡入。

### 图标

全站只有一种：**24×24 线框 SVG，`stroke-width:1.8`，圆角端点，颜色跟 `currentColor`。**
**不直出 emoji**（唯一例外：评分星 ★）。用 div 拼的字形（✓ 之类）一律换成 SVG 或 `clip-path`。

### 响应式

必须给 **≤1080 / ≤860 / ≤640** 三档降档，且**亮暗两套各看一遍**。少一档等于没做。

---

## 三、archetype 目录（零件名沿用 OpenFlow 短名）

| 用途 | 类 |
|---|---|
| 顶栏 | `.nav` › `.brand` `nav` |
| 首屏 · 居中 | `.hero-center` › `.kicker` `h1` `.lead` `.cta-row` `.trust` |
| 节标题 | `.sec-head` › `.kicker` `h2` `.lead` |
| 三栏 / 对照 | `.cols` › 子 `div` + `.ic` `h3` `p` |
| 混排网格 | `.bento` › `.card` + `.b-span4` `.b-span3` `.b-span2` |
| 大数字条 | `.stats` › `.st` `.st-n` `.st-t` |
| 工作流 | `.wf` › `.wf-step` `.wf-n` `h3` `p` |
| 玻璃窗 | `.sp-win` / `.hero-win` › `.win-bar`(`.light`×3 `.url`) + 内容 |
| 收尾 CTA | `.cta-band` › `.kicker` `h2` `.cta-row` |
| 手风琴 | `.faq` › `.fq` `.fq-q`(`.fx`) `.fq-a` |
| 页脚 | `.foot` › `.f-bottom` |
| 原子件 | `.btn`(`.primary` `.ghost`) `.card`(`.hov`) `.badge`(`.ok` `.warn` `.neutral`) `.pill`(`.hl`) `.ic` `.kicker` `.lead` `.note` `.mono` |

找不到合适零件时：先在这张表里找，找不到再考虑新增；新增的先进页面 `<style>`，等第二个页面要用再上收到 `ui.css`。

---

## 四、加一个页面

1. `<head>` 引 `assets/tokens.css` 与 `assets/ui.css`。
2. 页面私有 `<style>` 只写这一页独有部件，**含自己的三档降档**。
3. 亮色默认；需要暗色时在 `<html>` 上加 `data-theme="dark"`，不要另写一套颜色。
4. 提交前跑下面三条自检（见第五节），全 0 才算过。

---

## 五、自检命令（提交前必跑）

```bash
# 1. 页面里不许出现十六进制颜色
grep -nE '#[0-9a-fA-F]{3,8}\b' index.html assets/*.css && echo "✗ 有 hex" || echo "✓ 零 hex"

# 2. 不许出现 rgb()/rgba()
grep -nE '\brgba?\(' index.html assets/*.css && echo "✗ 有 rgb" || echo "✓ 零 rgb"

# 3. 三档断点齐全（应输出 1080 / 860 / 640）
grep -oE '@media\(max-width:([0-9]+)px\)' index.html | sort -u
```

外加两条人工检查：

- 图标是不是全部 24×24 SVG、`stroke-width:1.8`、没有 emoji 残留
- 切到暗色再走一遍，`--faint` 的小字还看得清吗

---

## 六、和 OpenFlow 的关系

| 项 | 状态 |
|---|---|
| 令牌 | 与 OpenFlow `assets/tokens.css` v6 **完全一致**，未改任何值 |
| 零件命名 | 沿用其短名风格（`.wf-n` 而非 `.timeline-node-number`） |
| 组件层 | WanFlow 目前只有落地页，`ui.css` 尚未抽；等第二页出现再上收 |
| 前端框架 | WanFlow 用 Vue 3 + Vite；契约与框架无关，`tokens.css` 直接可用，组件层按 Vue 单文件组件重写 |

**不要**为了"看起来不一样"自造一套配色。家族一致性的价值在于：五个产品放在一起，用户认得出是一家人。
