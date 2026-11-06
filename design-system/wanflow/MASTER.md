# Design System Master File

> **LOGIC:** When building a specific page, first check `design-system/pages/[page-name].md`.
> If that file exists, its rules **override** this Master file.
> If not, strictly follow the rules below.

---

**Project:** WanFlow
**Generated:** 2026-11-06 23:58:46
**Category:** Yoga & Stretching Guide

---

## Global Rules

### Color Palette

| Role | Hex | CSS Variable |
|------|-----|--------------|
| Primary | `#6B7280` | `--color-primary` |
| On Primary | `#FFFFFF` | `--color-on-primary` |
| Secondary | `#78716C` | `--color-secondary` |
| On Secondary | `#FFFFFF` | `--color-on-secondary` |
| Accent/CTA | `#0891B2` | `--color-accent` |
| On Accent/CTA | `#000000` | `--color-on-accent` |
| Background | `#F5F5F0` | `--color-background` |
| Foreground | `#0F172A` | `--color-foreground` |
| Card | `#FFFFFF` | `--color-card` |
| Card Foreground | `#0F172A` | `--color-card-foreground` |
| Muted | `#F6F6F7` | `--color-muted` |
| Muted Foreground | `#475569` | `--color-muted-foreground` |
| Border | `#EDEEEF` | `--color-border` |
| Destructive | `#DC2626` | `--color-destructive` |
| On Destructive | `#FFFFFF` | `--color-on-destructive` |
| Ring | `#6B7280` | `--color-ring` |

**Color Notes:** Sage neutral + calm teal

### Typography

- **Heading Font:** Lora
- **Body Font:** Raleway
- **Mood:** calm, wellness, health, relaxing, natural, organic
- **Google Fonts:** [Lora + Raleway](https://fonts.googleapis.com/css2?family=Lora:wght@400;500;600;700&family=Raleway:wght@300;400;500;600;700&display=swap)

**CSS Import:**
```css
@import url('https://fonts.googleapis.com/css2?family=Lora:wght@400;500;600;700&family=Raleway:wght@300;400;500;600;700&display=swap');
```

### Spacing Variables

| Token | Value | Usage |
|-------|-------|-------|
| `--space-xs` | `4px` / `0.25rem` | Tight gaps |
| `--space-sm` | `8px` / `0.5rem` | Icon gaps, inline spacing |
| `--space-md` | `16px` / `1rem` | Standard padding |
| `--space-lg` | `24px` / `1.5rem` | Section padding |
| `--space-xl` | `32px` / `2rem` | Large gaps |
| `--space-2xl` | `48px` / `3rem` | Section margins |
| `--space-3xl` | `64px` / `4rem` | Hero padding |

### Shadow Depths

| Level | Value | Usage |
|-------|-------|-------|
| `--shadow-sm` | `0 1px 2px rgba(0,0,0,0.05)` | Subtle lift |
| `--shadow-md` | `0 4px 6px rgba(0,0,0,0.1)` | Cards, buttons |
| `--shadow-lg` | `0 10px 15px rgba(0,0,0,0.1)` | Modals, dropdowns |
| `--shadow-xl` | `0 20px 25px rgba(0,0,0,0.15)` | Hero images, featured cards |

---

## Component Specs

### Buttons

```css
/* Primary Button */
.btn-primary {
  background: #0891B2;
  color: white;
  padding: 12px 24px;
  border-radius: 8px;
  font-weight: 600;
  transition: all 200ms ease;
  cursor: pointer;
}

.btn-primary:hover {
  opacity: 0.9;
  transform: translateY(-1px);
}

/* Secondary Button */
.btn-secondary {
  background: transparent;
  color: #6B7280;
  border: 2px solid #6B7280;
  padding: 12px 24px;
  border-radius: 8px;
  font-weight: 600;
  transition: all 200ms ease;
  cursor: pointer;
}
```

### Cards

```css
.card {
  background: #F5F5F0;
  border-radius: 12px;
  padding: 24px;
  box-shadow: var(--shadow-md);
  transition: all 200ms ease;
  cursor: pointer;
}

.card:hover {
  box-shadow: var(--shadow-lg);
  transform: translateY(-2px);
}
```

### Inputs

```css
.input {
  padding: 12px 16px;
  border: 1px solid #E2E8F0;
  border-radius: 8px;
  font-size: 16px;
  transition: border-color 200ms ease;
}

.input:focus {
  border-color: #6B7280;
  outline: none;
  box-shadow: 0 0 0 3px #6B728020;
}
```

### Modals

```css
.modal-overlay {
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
}

.modal {
  background: white;
  border-radius: 16px;
  padding: 32px;
  box-shadow: var(--shadow-xl);
  max-width: 500px;
  width: 90%;
}
```

---

## Style Guidelines

**Style:** Organic Biophilic

**Keywords:** Nature, organic shapes, green, sustainable, rounded, flowing, wellness, earthy, natural textures

**Best For:** Wellness apps, sustainability brands, eco products, health apps, meditation, organic food brands

**Key Effects:** Rounded corners (16-24px), organic curves (border-radius variations), natural shadows, flowing SVG shapes

### Page Pattern

**Pattern Name:** Hero + Testimonials + CTA

- **Conversion Strategy:** Social proof before CTA. Use a concise set of verified testimonials with photo, name, and role. CTA after social proof. Provide previous/next and pause controls; stop rotation on focus, hover, and reduced motion; announce slide position. Previous/next buttons and keyboard controls must expose every slide without dragging.
- **CTA Placement:** Hero (sticky) + Post-testimonials
- **Section Order:** Hero > Problem statement > Solution overview > Testimonials carousel > CTA

---

## Anti-Patterns (Do NOT Use)

- ❌ Inconsistent styling
- ❌ Poor contrast ratios

### Additional Forbidden Patterns

- ❌ **Emojis as icons** — Use SVG icons (Heroicons, Lucide, Simple Icons)
- ❌ **Missing cursor:pointer** — All clickable elements must have cursor:pointer
- ❌ **Layout-shifting hovers** — Avoid scale transforms that shift layout
- ❌ **Low contrast text** — Maintain 4.5:1 minimum contrast ratio
- ❌ **Instant state changes** — Always use transitions (150-300ms)
- ❌ **Invisible focus states** — Focus states must be visible for a11y

---

## Pre-Delivery Checklist

Before delivering any UI code, verify:

- [ ] No emojis used as icons (use SVG instead)
- [ ] All icons from consistent icon set (Heroicons/Lucide)
- [ ] `cursor-pointer` on all clickable elements
- [ ] Hover states with smooth transitions (150-300ms)
- [ ] Light mode: text contrast 4.5:1 minimum
- [ ] Focus states visible for keyboard navigation
- [ ] `prefers-reduced-motion` respected
- [ ] Responsive: 375px, 768px, 1024px, 1440px
- [ ] No content hidden behind fixed navbars
- [ ] No horizontal scroll on mobile

---

## WanFlow Adaptations（2026-11-06，人工收敛）

> 自动分类把本项目误判为「Yoga & Stretching Guide / wellness」。产品实际是**桌面端家庭照片整理与挑选工具**（Tauri + Vue 3，中文优先）。风格与配色采纳，动效与「有机曲线」按工具属性降级。

### 类别与取舍

| 项 | 生成结果 | 落地 |
|---|---|---|
| 类别 | Yoga & Stretching Guide（误判） | 桌面照片整理工具 |
| 风格 | Organic Biophilic（自然、有机、克制） | 采纳色板与气质；曲线 / 流动形状不采纳 |
| 模式 | Light 优先 | 双主题，深色仍为默认（暗房语境），浅色为暖纸底 |
| 强调色 | #0891B2 calm teal | 采纳；深色模式提亮为 #22B8D3 |
| 字体 | Lora + Raleway（Google Fonts） | 不联网取字体：落到 New York / 宋体、苹方 / Albert Sans，等宽继续 JetBrains Mono |
| 圆角 | 16–24px | 控件 10 / 卡片 12 / 容器 16 / 大面板 20（工具密度更高） |
| 投影 | natural shadows | 只给浮层（弹窗 / 命令面板 / Toast / 抽屉 / 灯箱 / Dock / 相纸 / 引导卡），列表与卡片用 0.5px 发丝 |

### Token 映射（src/tokens.css）

| MASTER | 本仓库 | 值 |
|---|---|---|
| --color-background | --bg-0 | light #f5f5f0 / dark #0c0e0e |
| --color-card | --surface-1 | light #ffffff / dark #161a19 |
| --color-foreground | --text-1 | light #0f172a / dark #f1f4f1 |
| --color-muted-foreground | --text-3 | light #475569 / dark #a6afa8 |
| --color-border | --border-1 | light #dcdcd2 / dark rgb(241 244 241 / .10) |
| --color-accent | --accent | light #0891b2 / dark #22b8d3 |
| --color-destructive | --danger | light #dc2626 / dark #f87171 |
| --space-* | --sp-1..8 | 4 / 8 / 12 / 16 / 24 / 32 / 48 / 64 |
| --shadow-sm/md/lg | --shadow-1 / --shadow-2 | 两档足够：浮层用小、面板 / 相纸用大 |

### 对比度实测（WCAG AA，普通文字 ≥4.5:1）

| 组合 | 比值 |
|---|---|
| dark --text-4 on --bg-0 | 6.19 |
| dark --text-2 on --surface-1 | 11.06 |
| dark --on-accent on --accent | 7.06 |
| light --text-4 on --bg-0 | 4.60 |
| light --text-3 on --surface-1 | 7.58 |
| light --accent-ink on white | 5.36 |
| light --on-accent on --accent | 4.54 |

### 未落地项（需要产品决策）

- 切浅色为默认：一行改动（theme 初值），本轮保持深色默认。
- 自托管 Lora / Raleway：离线产品可打包 woff2，但只影响拉丁字形，中文字形仍走系统衬线，收益有限。
