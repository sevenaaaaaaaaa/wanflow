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

---

## 2026-11-05 · DSH / lead —— 入口与上手引导：先玩一页，再谈几千张

**用户诉求**：「不够亮眼是因为交互和入口没有什么创新，没有引导用户用起来」。

**判断**：不是再加动效，而是三处断点：
1) 空状态只有一句说明 + 一个按钮，第一次打开看不出产品长什么样；
2) 有库以后顶栏 4 个平级动作（分析 / 精选 / 导入 / 命令），用户不知道下一步该点哪个；
3) 进了故事线也没有"跟着做一遍"的路径，键盘提示对新人等于没有。

**做了什么**（提交 `3ab32be`）：
- 空状态（`EmptyState.vue`）整页重写：左边一句话定位 + 两个并列入口（本地文件夹 / 照片图库），右边是**真能玩的一页**——点 1–6 改张数、版式跟着变、按「显影」出现"收进精选册"印章、可再玩一次；底部三步流程（导入 → 一天一页 → 显影）。入口点击直接打开对应 tab（`ImportModal` 增加 `initialTab`，App 用 `pickAndScan(kind)`）。
- 顶栏（`App.vue` + `styles.css`）：品牌旁只留一个动作入口「**继续整理 N 张**」（dot + 计数），没有待挑时变「去精选」并置灰；从网格点它会切回故事线；帮助按钮 `?` 重放引导。
- 跟随式引导（`Storyline.vue`）：4 步 spotlight（左边是日子 / 中间是这一页 / 决定留几张 / 按显影收工），Teleport 到 body，box-shadow 挖洞 + 240ms 位置过渡，卡片随焦点上下切换。关键不是"下一步"，而是**做一步自动前进**：点一天 → 第 2 步；改张数 → 第 3 步；按显影 → 结束。Esc / 先不看写 `wanflow-storyline-tour=done`；`?` 或顶栏帮助可重放；≤920px 时间轴隐藏时自动从第 2 步开始。
- `Icon.vue` 补 cloud/chat；`ImportModal` 的 tab 用图标而不是 emoji；`mock.ts` 加 `?empty` 空库演示（仅 DEV）。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build   # ✓ 53 modules，css 56.34 kB / js 173.30 kB
```
- 空状态 1440×900：doors=2、ticks=6、ph=4；点第 5 刻度 → 5 张；点显影 → 印章「收进精选册 / 这一页·5 张 / 再玩一次」；两个 door 分别打开「本地文件夹」「照片图库」tab。
- tour：自动开第 1 步 ring[4,48,194,860]；点一天 → 第 2 步 ring[304,146,773,584]；点下一步 → 第 3 步 ring[194,707,994,58]；点刻度 → 第 4 步 ring[995,707,107,58]；点显影 → 关闭且 done=done、days=9；按 `?` 重新开第 1 步；console error 0。
- CTA：正常库显示「继续整理 136」；切网格后点它回到故事线 10 天。

**踩过的坑**：
- 点"当前这一天"时 `activeIdx` 不变，`watch(activeIdx)` 不触发；必须在 `jump()` 里对 same-day 单独 `advanceTour()`。
- 只 `watch(currentBudget)` 时，点当前已选中的刻度不会前进；改成在 `setBudget()` 末尾判断 `tourStep===2`。
- 空状态显影层最初用 93% 米色整页盖住照片，像报错；改成 55% 纸色 + blur，只留中间印章卡。

**下一个动作**：`npm run tauri dev` 首次进入看引导是否自动出现（真实库 localStorage 可能已有旧 key）；然后按用户反馈决定是继续磨入口，还是回到"按人/地点/事件成页"的信息架构。

**回滚**：`git revert 3ab32be`。


---

## 2026-11-05 · DSH / lead —— 故事线视觉重做：把「一天一页」做成一张真正的相纸

**用户诉求**：把当前页面做到获奖级视觉（Awwwards / Webby / FWA 基准），按 typography / whitespace / hierarchy / color / motion / micro_interaction / responsive / originality 八维自查迭代，有 gap 就继续 patch。

**判断**：旧的表格布局信息都对，但"像后台表格"不像暗房。这一轮不加功能、不改逻辑，只换表达语言：
- **排版三分**：展示层用衬线（New York / 宋体）——大日期 58–76px、纸面落款、精选册标题；元数据用等宽 8.5–10px（页码、时长、页脚）；正文仍用 Albert Sans。三级字号从此各有职责。
- **相纸**：暖米白 `--paper: oklch(97% .021 88)` + 180px feTurbulence 纤维 + 四角印刷角标 + 灯箱暖光 + 接触阴影；照片衬在纸上，而不是浮在暗底。
- **显影**：照片载入 860ms 从 `blur(11px) saturate(.32)` 显到清晰，按 70ms 递增；按「显影」时纸面轻微收缩 + 中央白光 300ms，然后翻页。
- **底片条**：96×70 缩略、左下角编号、选中绿环、拖拽时整条变暗、被拖那张反亮；右栏空册改成一张微微歪着的纸（-0.7°），显影后逐页叠上（±0.5° 交替）。
- **窄屏**：≤920px 左侧时间轴收成顶部横滑「日签」（日期 + 周几 + 张数），不从功能上消失；≤760 compose 换行；≤560 再收元数据。

**改了什么**（提交 `d5e19d2`，3 个文件 +615/−272）：
- `src/tokens.css`：新增 `--fs-6/--fs-7/--fs-day`、`--font-display`、一整套纸感变量；`--text-4` 暗色 61.5%→67%、亮色 57%→49%（10–11px 小字过 4.5:1）。
- `src/styles.css`：字标改衬线（Flow 斜体绿）；「继续整理」从实心绿改描边（让「显影」成为页面唯一实心绿）；容量 chip 改等宽仪表字；顶栏在 1000/900/640 三档依次收起文案/容量/次要入口（修 768 中文逐字换行）。
- `src/components/Storyline.vue`：模板加 `.day-rail`（窄屏日签）、`.day-sub`、`.pagemark`、`.ticks-wrap`、`.f-no`、`.ghost-mark`；样式重写页眉/相纸/compose/底片条/精选册/引导卡；`prefers-reduced-motion` 下全部停用。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build   # ✓ exit 0，53 modules，css 66.77 kB / js 174.07 kB，724ms
```
- 1440×900（/tmp/wanflow-final4.png）：大日期 05 + 页眉横线 + 700×525 相纸；compose 距纸底约 22px；右册 ghost 居中。
- 1024（/tmp/wanflow-1024.png）：右册让位，页眉出现「已显影 0」。
- 768（/tmp/wanflow-768b.png）：顶栏不再逐字换行；日签横滑正常；`scrollHeight=900`，无纵向溢出。
- 引导态（/tmp/wanflow-tour.png）：卡片与页面同一套相纸语言。
- `git status` 提交前只有这 3 个文件；提交后 clean。

**已知小债**：
- 组件里仍有若干裸 px 字号（9.5/8.5/10），与 tokens.css「禁止裸 px 字号」的约定不完全一致；下一轮可加 `--fs-0` 统一收口。
- 相纸在真实照片下的白平衡/密度/颗粒强度还没校（DEV 样片是灰调 SVG）。

**下一个动作**：`npm run tauri dev` 用真实照片看这一版；如果「纸」的比喻成立，再把成页逻辑从日期扩到事件/地点/人。

**回滚**：`git revert d5e19d2`。


---

## 2026-11-05 · 故事线支持按事件成页（纯前端）

**判断**：日期是线性的，「地点 / 人」不是——它们必须先当筛子再分页；能直接当「页」的只有**事件**。而事件不需要新数据：effTime 已经在库里。所以先把成页单位从日历天扩成「一段连续拍摄」，成页逻辑重构但不碰 Rust / DB / 原图。

**改动**：
- `src/components/Storyline.vue`
  - `Day` / `Page` 增加 `endTs`（这一页最后一张的时间）；新增 `groupByDay(list)` 与 `groupByEvent(list)`：升序扫描，相邻两张间隔 > 6 小时（`EVENT_GAP_SEC = 6 * 3600`）就断开，跨午夜连续拍自然合并；key = `evt-<首张 id>`。`days` 按 `groupMode`（localStorage `wanflow-group-mode`）从两者里取。
  - 精选册改记「显影时的页」而不是日期：新 key `wanflow-developed-pages`（`{ ids, ts, endTs, mode }`），旧 `wanflow-developed-days` 一次性迁移；`rememberPage(page, ids)` 写入，新页完整覆盖旧页时把旧页换掉，避免同一批照片在册子里出现两次；`developed` 只在 ids 全部仍是 keep 时显示（⌘Z/⌘⇧Z 同步）。
  - 文案与层级：左栏加 `.group-switch`（按天 / 按事件，aria-pressed）；事件模式下时间线每行显示开始时间 `HH:MM`（同一天可有多段）；页头副标题给 `spanLine`（同日 `18:17 – 18:40`，跨日 `10/31 23:28 → 11/01 00:08 · 2 天`）；纸脚/册子标签走 `pageLabel`（同日「M 月 D 日」，跨日「M/D – M/D」）；「这一天/天」按模式切「这一段/段」；切换时以当前页起点为锚，不跳页。
  - 窄屏（≤920px）时间线隐藏时，日签行左侧加 `.rail-switch`（sticky），成页方式仍可切。
  - 顺手修窄屏相纸溢出：≤920 纸宽改 `min(100%, calc((100vh - 440px) * 1.3333))`，之前 `width:100%` 会压到 compose。
- `src/dev/mock.ts`（仅演示库）：把 ago=1 / ago=10 两天拆成上午 + 傍晚两场，ago=5 改成 23:20 起拍、跨过午夜（23:28 → 00:08），用来肉眼验证「按天拆两行 / 按事件仍一段」。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build   # ✓ exit 0，53 modules，css 68.17 kB / js 177.92 kB，761ms
```
- 1440×900 按天（/tmp/wanflow-day-final.png）：11 天，左栏开关「按天」选中，其余与相纸版一致。
- 1440×900 按事件（/tmp/wanflow-event-final.png）：12 个事件；10/31 23:28 → 11/01 00:08 · 2 天（跨午夜合并成一段）；纸脚「10/31 – 11/01 · 2 天」。
- 768×900（/tmp/wanflow-768-event2.png）：日签行左侧「按天/按事件」可切；相纸、compose、胶片条不再互相压；`scrollHeight=900`，无纵向溢出。
- 交互实测：显影跨午夜那段 → 精选册 mini 显示「10/31 – 11/01 · 2 张」，事件数 12→11、已显影 1/12 段；⌘Z 后册子清空、事件回到 12；⌘⇧Z 后册子与标签都回来。
- 切模式不跳页：11/05 事件页 → 按天仍是 11/05；10/31 23:28 事件 → 按天仍是 10/31。

**已知**：
- 6 小时阈值是拍的脑袋：真实库里「出门一天」可能超过 6 小时导致断成两段，等真实数据反馈再调。
- 精选册只保证「新页完整覆盖旧页」时去重；两页部分重叠（把别页照片拖进来后各自显影）仍可能同一张照片出现在两页。
- 演示库时间被改过（两场日 + 跨午夜夜拍），真实库不受影响。

**回滚**：`git revert <feat sha>`（mock 与 Storyline 在同一个 feat 提交里，一起退回按天逻辑）。

## 2026-11-05 · 事件边界加设备维度（2–6h 换设备即断开）

**判断**：6 小时一刀切会把「下午出门一趟、傍晚回家」并成一段，也会把「同一场拍摄里换机位」拆开。回头查了数据面：media 表（`src-tauri/src/db.rs:12-30`）没有 GPS、没有相机 Make/Model；`src-tauri/src/timefix.rs:42-53` 只读 DateTimeOriginal/DateTime；phash 已入库（`src-tauri/src/phash.rs:1-20`）但 `MEDIA_COLS`（`src-tauri/src/commands.rs:641-645`）没 SELECT 出来；CLIP 只做语义搜索、没有嵌入表。所以这轮只做「不新增数据也能判」的那一维：**文件名前缀当设备指纹**（IMG_/PXL_/Screenshot_）。地点、内容相似性等真有数据了再上。

**改动**（纯前端）：
- `src/components/Storyline.vue`：`EVENT_GAP_MAX = 6h`、`EVENT_GAP_MIN = 2h`；`filePrefix()` 取文件名前缀小写；`isSessionBreak(prev,next)`：同连拍组 → 不拆；> 6h → 拆；≤ 2h → 不拆；2–6h 之间 **前缀变了才拆**。toast/按钮 title 同步新规则。
- `src/dev/mock.ts`：新增 `DEVICE_DAY = 3`（11/02）下午换成安卓机拍（PXL_ 前缀、14:09 起）；并修了一个演示数据的坑——连拍组原本会跨设备边界，导致「同连拍组永不拆」把设备断点吃掉，现在换设备时重置连拍组。

**验证**：
```bash
cd /Users/seveno/WanFlow && npm run build   # ✓ exit 0，53 modules，css 68.17 kB / js 178.37 kB，756ms
```
- 1440×900 按天（/tmp/wanflow-device-day.png）：11 天，11/02 仍是 19 张一行。
- 1440×900 按事件（/tmp/wanflow-device-event.png）：13 个事件，11/02 拆成 18:22（11 张）与 14:09（8 张）；toast 文案已是「超过 6 小时断开；2–6 小时间换了设备也断开」。
- 页面内取数核对：`api.listMedia({})` 159 张里 PXL_ 前缀 9 张；分组边界对的 burstGroup 均为 null、间隔 14588s（4h03m）→ 前缀不同 → 拆开。
- 中间用 `window.__dbg` 临时打点定位到「连拍组跨设备」这个真因，验证完已删除，构建产物无残留。

**已知**：
- 前缀只是设备指纹的粗代理：同机不同 App、微信保存图、改名后导出都会让前缀漂移；真相机型号要加 Rust/DB 列（Make/Model）才准。
- 6h / 2h 两个阈值仍是拍的；地点（GPS）与内容相似性（需持久化 CLIP 嵌入）都没数据。

**回滚**：`git revert <feat sha>`（Storyline + mock 同一提交，退回纯 6h 逻辑）。

## 2026-11-05 · 场景聚类引擎 + 事件层「设备拆分」纠错

**判断**：
- 上一轮的设备前缀拆分与产品北极星冲突：家人用不同手机拍同一幕，间隔落在 2–6h 时会被强行拆开。设备只能当弱证据，不能当断开规则；事件层回退为纯 6h 间隔。
- 「同一天、跨设备、同场景」只能靠内容。第一步落引擎：`src-tauri/src/scene.rs` 纯函数 `cluster`，同场景 = 相邻间隔 ≤ 90min 且与场景质心余弦 ≥ 0.82，且整段跨度 ≤ 6h；任一侧无 embedding 时退回更严的 30min 时间判断。跨午夜连续拍摄自然并成一段（只看间隔/跨度，不强制按日历天切）。
- 场景键 = 场景首张 media id，重算稳定；结果写 `media.scene_group`，可随时重算，不动原图。

**改动**：
- 新增 `src-tauri/src/scene.rs`：`SceneItem{id,ts,emb}`、`SceneParams`（90min / 6h / 0.82 / 30min）、`cluster`、`rebuild`（事务内重算并写 `scene_group`，返回 `scenes/images/withEmb/missingEmb/changed`）、`rebuild_scenes` 命令；13 个单测覆盖合并 / 内容断开 / 间隔 / 跨度 / 跨午夜 / 次日 / 无 emb / 维度不一致 / 乱序确定性 / 落库幂等 / 旧值清理。
- `clip.rs`：`vec_to_blob` / `blob_to_vec` 从 `smart.rs` 提出来（embedding 落库格式单一来源）；`smart.rs` 改为引用。
- `db.rs`：`ensure_columns` 增 `scene_group`，加 `idx_media_scene`。
- `lib.rs`：注册 `scene::rebuild_scenes`（第 34 个命令）。
- `Storyline.vue` / `mock.ts`：移除 `EVENT_GAP_MIN`、`filePrefix`、设备前缀判断与安卓演示日；toast / title 文案同步；事件边界只剩 6h 与「同连拍组不拆」。

**验证**：
```bash
cd /Users/seveno/WanFlow/src-tauri && cargo test   # ✓ 53 passed（原 40 + 场景 13）
cd /Users/seveno/WanFlow && npm run build          # ✓ exit 0，53 modules，css 68.17 kB / js 178.06 kB，716ms
```
- 浏览器 1440×900（/tmp/wanflow-event-no-device.png）：按事件 13 → **12 段**，11/02 由 18:22 + 14:09 两段并回 `18:08` 一段 19 张；按天仍 11 天；toast 已是「超过 6 小时断开；跨午夜连续拍仍算一段。换手机不会断开。」
- 场景落库路径用内存 SQLite 测过：4 张分两场景、`scene_group` 写入 1/1/3/3、重复 rebuild `changed=0`、旧场景值被清空。

**已知 / 风险**：
- 真实库（`~/Library/Caches/com.wanflow.app/wanflow.db`）30 行、`emb` 0/30、CLIP 模型未下载（需 `clip-vit-b32-q8.onnx` + tokenizer，约 150 MB）→ 0.82 / 90min / 6h 都还没有真实照片校准；这批还是合成测试图。
- `scene_group` 尚未暴露给 `MediaItem`，故事线还没有「按场景」——接 UI 前，这个引擎在应用里不可见。
- CLIP ViT-B/32 对「同场景不同角度」的分辨力是最大未验证风险；若阈值分不开，需要换更大模型或补 phash / 地点信号。
- `rebuild` 清理旧场景值用 `json_each`（SQLite JSON1，bundled 版本内置，测试已通过）。

**回滚**：`git revert 84768ae a056b65`（先退事件层修复，再退引擎；`scene_group` 列留在库里不影响旧代码）。
