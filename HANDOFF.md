# HANDOFF.md —— 交接记录（只追加，不改写）

> 每次停下（收工、换设备、换 agent）就在这里追加一段。最新的一段放最前面。
> 目的：接手的人不用问任何人，就知道"上一个人为什么停在这"。

---

## 交接模板（复制这段到最上面）

```
### 2026-MM-DD HH:mm · <agent 名> · <设备>

**做到哪**：<一句话，具体到文件和功能，不要写"做了一半">

**为什么停**：<完成 / 等确认 / 卡在报错 / 时间到>

**下一个动作**：<下一个人第一件事做什么，精确到命令或文件>

**别踩**：<这次踩过的坑，一条一行；没有就写"无">

**验证方式**：<怎么确认这次改动是好的，给命令>

**未提交的改动**：<有就列出文件；没有写"无">
```

---

## 2026-10-25 · 建立模板

**做到哪**：初始化 `AGENTS.md` / `STATE.md` / `HANDOFF.md` 三个文件。

**为什么停**：模板建好，等接入具体项目。

**下一个动作**：`bash install.sh /path/to/project`，然后填 `AGENTS.md` 的技术栈表和 `STATE.md` 的一句话现状。

**别踩**：项目里若已有 `CLAUDE.md`，脚本不会覆盖它 —— 要手动把两者的内容合并，或者把原有内容并进 `AGENTS.md` 再建软链。

**验证方式**：`git status` 应只多出这三个文件 + `.gitignore` 的一行。

**未提交的改动**：无。


---

## 2026-11-04 · DSH / lead —— 精选集导出（M6 的第一块）

**做到哪**：`src-tauri/src/export.rs` 从零实现「导出精选集」并接进 UI：`export_preview`（dry-run）/ `export_apply`（spawn_blocking + `export://progress|done|error`）/ `export_status`。
- 范围：`album`（精选集）/ `keeps`（所有保留）/ `selection`（当前选中）；可含/不含视频。
- 目录结构：`flat` / `year` / `month` / `album`；命名模板占位符 `{date}{time}{yyyy}{mm}{dd}{album}{seq:NN}{orig}{stars}`，文件名非法字符替换、`..` 压平、重名加序号、跨文件重名去重。
- 元数据：默认逐字节复制；`strip_metadata` 删 JPEG APP1/Exif（含 GPS）；`rewrite_time` 就地改写 `0x0132/0x9003/0x9004`（仅 type=2、count=20 的项），非 JPEG 原样复制并在 note 里标注。
- 幂等：目标目录写 `.wanflow-export.json` 清单（src→dest），重复运行直接跳过，不会造 `_2/_3`。
- 安全：`dest_inside_sources()` 拒绝把导出目录放进任一来源根目录；原图只 `fs::read`，从不写入。
- 前端：`src/components/ExportModal.vue`（范围/目录/布局/模板/复选框 + dry-run 表格 + 进度条），`src/api.ts` 增 4 个类型 + 3 个方法，`src/App.vue` 顶栏「导出」按钮 + 挂载 + toast。
- 测试：新增 10 个（含端到端 `plan_and_execute_copies_without_touching_source`、`refuses_dest_inside_source_dir`），全库 40 个全绿。

**为什么停**：功能完成且 `cargo test` / `npm run build` 通过；GUI 里的人工点选验收还没做（需要 `npm run tauri dev`）。

**下一个动作**：`npm run tauri dev`，选几张照片 → 顶栏「导出」→ 选目录 → 预览 → 导出；确认进度条、`.wanflow-export.json` 生成、再导一次全部跳过。

**别踩**：
- `render_name` 的 `{seq:04}` 必须用 `expand_seq()` 处理，若只在 `pairs` 里放 `{seq}`，模板会原样残留占位符。
- 导出幂等不能只靠"目标同大小"判断：剥离 EXIF 后副本比原图小，会把每次重跑都当冲突。清单文件才是准的。
- `export_apply` 里必须自开 `db::init(&cache.join("wanflow.db"))`（不能用 `State` 的连接，跨线程）。

**验证方式**：
```bash
cd /Users/seveno/WanFlow/src-tauri && cargo test            # 40 passed
cd /Users/seveno/WanFlow && npm run build                    # ✓ built
```

**未提交的改动**：本次提交前无（提交后 `git status` 应为干净）。

---

## 2026-11-04 · DSH / lead —— 交互重构（Litmus 风格）+ 视觉修正

**用户诉求**：「目前就是交互太差了，你可以学学 litmus 的设计」，随后追加「审美太差」。

**做了什么**（两个提交，均已推送）：
- `7637298` `feat: 交互重构（Litmus 风格视觉 + 键盘优先挑图 + 命令面板 + 浮动 Dock）`
  - 新增 `src/tokens.css`：oklch token 体系（表面五级/文字四级/语义四色/圆角五档/动效五档/玻璃与投影），暗色默认 + `[data-theme="light"]`。
  - 重写 `src/styles.css`（约 20KB）：顶栏、分段控件、网格 cell、浮动 Dock 动作栏、命令面板、灯箱、弹窗、撤销 toast；保留旧 token 别名让未改动的组件不破。
  - 新增 `src/components/CommandPalette.vue`（⌘K / `/`，↑↓↵esc，分组 + 快捷键列）、`src/components/Icon.vue`（16 网格线性图标）、`src/types.ts`（`Cmd` 接口）。
  - 重写 `PhotoGrid.vue`：虚拟网格 + `focusIdx/anchor`，方向键/Home/End/PageUp/PageDown 移动，Shift 扩选，Space 切换选中，P/X/U 判定，0-5 加星，Enter 开灯箱；悬停出 留/弃/星 快捷按钮。
  - `App.vue`：底栏有选区时变动作栏（N 张选中 / 保留 / 待定 / 淘汰 / 1-5 星 / 修正时间 / 导出 / 取消），无选区时显示统计 + 快捷键提示；全局 `closeTop()` 统一 Esc 关浮层栈；主题写 `localStorage("wanflow-theme")`。
- `c480864` `style: 提高浅色主题对比度、状态角标改为语义色点 + 实心胶囊、分段控件加容器`

**踩过的坑**：
- 浮层"点一下就消失"：遮罩用 `@click.self` 时，打开浮层的同一次 mousedown→click 会落到刚挂载的遮罩上。全部 8 个浮层改成 `@mousedown.self` 才对。
- `<script setup>` 里不能 `export interface`，共享类型放 `src/types.ts`。
- 前端仍无 `vue-tsc`：`npx tsc --noEmit` 会报 `Cannot find module './App.vue'`，属既有问题。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build     # ✓ built，css 27.07 kB / js 136.69 kB
```
GUI 手测（`npm run tauri dev`）：30 张真实照片的演示库，⌘K 面板开合、方向键移动、点选后 Dock 变动作栏、状态角标显示均正常。

**下一个动作**：按用户对审美的进一步意见继续调（候选：网格改瀑布流保留原始比例 / 顶栏收纳 / 空状态重做）。

**别踩**：`applyTheme()` 在 `onMounted` 会覆盖临时设置的 `documentElement.dataset.theme`，临时预览主题要在 setTimeout 里再设，或直接改 `localStorage`。
