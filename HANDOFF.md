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

---

## 2026-11-05 · DSH / lead —— 故事线：一天一页，定量显影

**用户诉求**：「你的交互还不够创新，视觉有点提升了，但是不够创新」。

**核心动作**：不再优化"逐张判定"，改成新的心智模型——**一天一页，给这一页定量显影**。
- 左栏是时间轴（按月份分组、圆点大小=当天张数、当前日高亮），中栏是当天的"相册页"，右栏是"精选册"（已显影的 mini 页堆叠）。
- 页面不是固定拼图：用「这一页，留几张？」的刻度尺（1–6，可点可拖，也能按 1–6 / ←→）决定当天保留数量，系统按 星级×14 + 清晰度×0.7 + 连拍最佳 + 时间置信度 自动排一页；页面会随张数变成 1/2/3/4/5/6 张的不同版式。
- 不满意自动选择：点页内照片「换掉」＝移出并自动补位；点胶片帧＝加入/移出；**把胶片帧拖进页面**＝钉住这张并抬预算（webview 里 dataTransfer 可能失效，已用 dragstart 记下的 id 兜底）。
- Enter / 「显影」→ 快门白闪 → 页面飞进右侧精选册 → 自动翻到下一天；X 跳过当天；R 重置人工选择。
- 精选册与撤销一致：册页不是本地记账，而是 `kept` 重新按天分组得出；只显示本轮显影过的天（localStorage `wanflow-developed-days`），历史 keep 不会冒充本轮成果。⌘Z/⇧⌘Z 可整组撤销/重做一次显影。
- 新增 Rust 命令 `develop_page(keep_ids, rest_ids)`：单事务写一次 action + 批量 verdicts。原因：原来两次 `set_decision` 是两个撤销批次，⌘Z 只回滚一半；且 `redo_last` 的 SQL 无法按组重做。
- 细节：双纸堆叠（下层纸旋转 -0.9°）、翻页 3D 动画（440ms，尊重 reduced-motion）、顶栏 toast（不挡页眉）、浏览器演示库（`src/dev/mock.ts`，仅 DEV 且无 Tauri 时启用）。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build     # ✓ built，css 46.26 kB / js 162.80 kB
cd /Users/seveno/WanFlow/src-tauri && cargo test   # 40 passed; 0 failed
```
CDP 实测（1440×900，headless）：初始 10 天 / 15 张待挑；显影后 9 天、精选册 18 张、页码 04、toast「显影 4 张 · 其余 11 张转待定」；⌘Z 回到 10 天 / 14 张 / 第 1 页，⇧⌘Z 再回到 9 天 / 18 张 / 第 4 页。拖拽测试：页面首图 = 拖入那张（first=true）。网格视图回归：30 个 cell、Dock、排序均在。

**踩过的坑**：
- `.spread` 和 `.paper-under` **都要** `grid-area:1/1`，只给下层纸设会让正页被自动排到第二行，整页塌下去。
- 合成 `DragEvent` 时 `dataTransfer.getData` 可能为空；必须用 dragstart 记录的 `dragId` 兜底，否则真实拖拽也可能受 webview 影响。
- 全局裸 button 规则曾用 5 个 `:not` 把特异性抬到 0,5,1，压过 scoped 样式（.frame 被压成 30px）；已改成 `:where(...)` 降为 0。
- 显影必须是**一个** action，否则撤销只能撤一半。

**下一个动作**：`npm run tauri dev` 用真实库试用 ⌘1 故事线；再按反馈调"张数刻度"和"换掉"的手感。

**回滚**：`git revert 0072d5a`。

---

## 2026-11-05 · DSH / lead —— 去特效：单张纸 + 摄影感演示图

**用户诉求**：「照片的魔法特效、堆叠特效不太好」。

**判断**：功能没坏，坏在两种"假"：
1) 演示照片像插画（高饱和渐变 + 发光大球 + 山），不是照片；
2) 页面像舞台道具（纸下再垫一张旋转的纸、精选册 mini 页歪着、显影后整页飞向右侧、快门白光）。

**做了什么**（提交 `de37a90`）：
- `src/components/Storyline.vue`：删除 `.paper-under` 与整段 CSS，页面只留一张纸（内高光 + 两层柔和投影）；`pageTurn` 改为 260ms 淡入 + 上移 7px（去掉 rotateY / transform-origin）；显影从"飞进精选册"改为淡出 220ms 后重读；删除 `.shutter` 白闪；精选册 mini 页去掉 `--rot` 旋转与弹跳，ghost page 也不再倾斜。显影成功后仍走原子命令 `develop_page`，⌘Z/⇧⌘Z 整组撤销/重做不变。
- `src/dev/mock.ts`：重写 `photo()` 为 5 个摄影场景（暮色山脊 / 夜色散景 / 窗边逆光 / 海面薄雾 / 林间散景），每场景 2 个配色变体；seeded PRNG 固定构图，统一 110 颗颗粒 + 暗角。窗景有窗棂、窗台、锚在左下角的前景失焦绿植；林景有树冠阴影与两侧柔边树干。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build   # ✓ 52 modules，css 45.40 kB / js 162.63 kB
node /tmp/wf-look.mjs                        # CDP 1440×900
```
- 初始：10 天 / 15 张 / 4 槽 / 0 mini / 无 shutter / 无 paper-under；点显影后 9 天、1 mini、toast「显影 4 张 · 其余 11 张转待定」。
- 截图：/tmp/wf-look1.png（页面）、/tmp/wf-look2.png（显影后）；单张样片 /tmp/wf-forest.png、/tmp/wf-one-2.png（窗景）。
- 生产构建里 mock 被 `import.meta.env.PROD` 短路并 tree-shake，所以改演示图不会改 bundle hash（属预期，不是没生效）。

**踩过的坑**：
- 暗背景上叠 4% 平涂大椭圆会露出硬边，像镜头污渍；改成"树冠渐变 + 柔边树干"。
- 窗景只有竖向帘影会像白墙；加窗棂/窗台/前景绿植后，才读得出是室内。
- `git add -A` 依旧禁用；本轮只提交 `src/components/Storyline.vue` 与 `src/dev/mock.ts`。

**下一个动作**：`npm run tauri dev` 用真实照片看故事线。如果仍觉得"不够创新"，下一刀建议动信息架构（按人/地点/事件成页，而不是日期），不要再加动效。

**补记（同日）**：CDP 回归发现撤销显影后精选册页还在（`kept` 回退到 14 张，但 `developedDays` 只记了日期，页仍按当天历史 keep 渲染）。改成 `Record<dayKey, keepId[]>`：显影时记下这一页的 ids，精选册只显示「这些 ids 现在仍是 keep」的页；撤销后自然消失，重做后自动回来。旧 localStorage 数组自动迁移。复测 START 10 天/0 mini → 显影 9 天/1 mini/18 张 → ⌘Z 10 天/**0 mini**/14 张 → ⇧⌘Z 9 天/1 mini/18 张。

**回滚**：`git revert de37a90`。
