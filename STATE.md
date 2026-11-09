# STATE.md —— 当前状态（每轮开工先读这个）

> 规则：只写"现在的真实情况"，不写历史。历史进 `HANDOFF.md`。
> 每次收工前更新三处：版本、进行中、下一步。旧内容删掉，不要堆积。

**最后更新**：2026-11-09 · 更新人：DSH / lead · 设备：开发机（macOS）

## 一句话现状

v0.2.2（2026-11-09：审计整改 Top1-3 + 进度提示改线框图标 + 版本号抬到 0.2.2；本地 `/Applications/WanFlow.app` 已换成 0.2.2；dmg `WanFlow_0.2.2_aarch64.dmg` 18.0 MiB、sha256 `f27d34bb…`；已推 main、打 tag v0.2.2、发 GitHub Release（Latest）、main 上 CI 绿（14s））。上一版 v0.2.1（2026-11-07 已发版，arm64 dmg，未签名/未公证；暖纸 + 青的视觉、浅色默认；修了欢迎页版本号写死）。**从导入到交付的闭环已打通**：导入（文件夹 / 照片图库 / WebDAV）→ 扫描 → 质量分析 → 去重与连拍分组 → 初筛建议 → 人工决策 → 智能精选 / 语义搜索 → **精选集导出**（dry-run 预览 + 副本导出，支持目录结构、命名模板、剥离 EXIF、改写时间、导出清单幂等）。
规模：Rust `src-tauri/src` 5754 行 / 前端 `src` 5743 行；**34 个 Tauri 命令**；`cargo test` **53 个测试全绿**（另有 `tests/scene_calibration.rs` 手工校准工具，默认 ignored）；`npm run build` 通过。
交互：**故事线三种成页——按天 / 按事件（边界 = 6h 间隔，设备不参与）/ 按场景**（场景键读 `media.scene_group`；没有索引时回退事件并提示，左栏有「重建场景」入口；真实库已建索引：30/30 张有向量、30 场——这 30 张彼此无关，每张一场）+ 定量显影 + Litmus 风格 token（`src/tokens.css`）+ 键盘优先挑图 + ⌘K 命令面板 + 浮动 Dock 动作栏 + 明暗双主题。视觉已去特效：**单张纸**、无快门白闪 / 整页飞行 / 旋转堆叠；浏览器演示库改用摄影感样片（仅 DEV，生产构建 tree-shake）。2026-11-06 又按微件视觉规范做了一轮**扁平化**（`52ac1c5`）：去掉投影 / 毛玻璃 / 背景辉光 / 相纸颗粒与显影白闪，字重只留 400 与 500，最小字号 11px，边框统一 0.5px；浮层改不透明表面色。同日晚些时候按 `ui-ux-pro-max` 生成了设计系统并落盘 `design-system/wanflow/MASTER.md`（Organic Biophilic：sage neutral + calm teal），`src/tokens.css` 换成暖纸底 / 中性石墨 / 单一青强调，投影只回给浮层（弹窗、命令面板、抽屉、灯箱、Dock、相纸、引导卡），两套主题按同一色板对齐。2026-11-07 把浅色设为默认（首帧定主题，不闪深色），版本升 0.2.0 并发版。
入口与引导：空状态**可直接玩一页**（改张数/显影/再玩一次），两个导入入口直接落到对应 tab；顶栏只保留一个动作入口「**继续整理 N 张**」；首次运行自动开 **4 步跟随式引导**（做一步自动前进，`?` 重放，Esc/先不看写 localStorage）。
蓝图 v0.3 里的在线协同（CRDT）、`.wanpack` 整库打包、本地加密库**尚未开始**。

## 进行中

| 事项 | 状态 | 卡在哪 | 谁在做 |
|---|---|---|---|
| 故事线视图 | 0072d5a 基础版 + de37a90 去特效版，等用户试用反馈 | 需要 `npm run tauri dev` 真实库手测；演示样片仍是 SVG 生成，真实照片下才能判断 | DSH / lead |
| 入口与上手引导 | 3ab32be 已提交，等用户试用反馈 | 首次运行 tour 在真实库（无 `?empty`）下是否还会被已有 localStorage 跳过 | DSH / lead |
| 故事线视觉语言 | d5e19d2 相纸/暗房版已提交，等用户试用 | 截图用的是 DEV SVG 样片；真实照片下纸面白平衡、照片密度、颗粒强度还要再校 | DSH / lead |
| 场景聚类 | 引擎 a056b65 + UI 2893e41 已提交；真实库已下 CLIP 模型并嵌入 30/30、重建 30 场（二次 rebuild `changed=0`）；校准工具 21f4bb9 | 真实库 30 张是彼此无关的风景照，**没有同场景多拍**，跨设备合并率只能用代理变体测（0.82 时 85.6%、误并 0）；需要真实家庭照片定稿 | DSH / lead |
| 视觉扁平化 | 52ac1c5 已提交（`npm run build` 通过 + 浏览器截图核对），等用户试用 | 需 `npm run tauri dev` 在真实照片下看三处：相纸白平衡、悬浮层分界、11px 小字 | DSH / lead |
| 设计系统接入 | 已发版 0.2.0：浅色默认 + 暖纸/青；`design-system/wanflow/MASTER.md` 为唯一口径 | 真实照片下看青强调是否太亮、浅色下相纸与底色的分层是否够 | DSH / lead |
| 家人 / 人脸 / AI 功能线（恢复合并） | 已合并到 v0.2.1 基线（3346fd4 快照 → a743b26 合并）；cargo test 54 绿、npm run build 通过；顶栏「家人」入口 + FamilyPanel 就位 | 需真机 `npm run tauri dev` 验收：人脸聚类质量、家人注册/按人筛选、AI 精选（agent_select 走 Ollama）；未发版 | DSH / lead（2026-11-07） |
| 应用内检查更新 | 已实现并冒烟：⌘K「帮助 → 检查更新…」+ 欢迎页版本行入口；匿名调 GitHub Release 比对 `__APP_VERSION__`；截图 docs/shots/smoke-update-check.png | 真机验收；随下次发版生效 | DSH / lead（2026-11-07） |
| 视觉审计整改（unslop × landing-page-design） | Top1-3 已完成（903a4fd）：面板 emoji→线框图标 13 处（新增 6 个图标）、非 token 色归位（#fca5a5→--danger-ink、#fff→--on-accent、Arena 旧主题迁移）、动效收敛（删弹簧/弹跳曲线、悬停位移 ≤2px、入场改淡入、--dur-2 对齐 200ms）；`npm run build` 通过 | Top4（间距/字号尺度化）与 Top5（演示样片假照片）未做，需单独排期 | DSH / lead（2026-11-09） |
| v0.2.2 收尾 | 1a7a1d4：进度提示 ⏳ → `Icon time`（FamilyPanel/SmartPanel 各 1 处），package / package-lock / tauri.conf / Cargo 四处置 0.2.2；`npm run build` 1.50s 通过、`src/` 内 ⏳ 残留 0；release 产物 `WanFlow.app` 48M（Info.plist 0.2.2）、dmg 18.0 MiB 校验通过；`/Applications/WanFlow.app` 已替换并启动（2 窗口：主窗 1360×860） | 真机点一遍进度提示外观；**push 分支 / 合并 main / 发 GitHub Release 待授权** | DSH / lead（2026-11-09） |

## 下一步（按先后顺序）

1. **用真实家庭照片校准场景阈值**：准备一批含「同场景多拍 / 跨设备同一天」的照片 + `labels.csv`，跑 `WF_CALIB_DIR=<目录> cargo test --test scene_calibration -- --ignored --nocapture`；按合并率 ≥80%、误并 ≤10% 选 `min_sim / max_gap / max_span`，再改 `SceneParams` 默认值并重建场景。
2. **试用入口与引导**：`npm run tauri dev` → 首次进入应自动出现 4 步引导；跟着点一天、改张数、按显影，看它是否自动前进；`?` 可重放。再到空库（浏览器 `/?empty`）玩一遍右侧示例页，确认两个导入按钮各自落到正确 tab。
3. **试用故事线**：`npm run tauri dev` → 顶栏「故事线」/⌘1 → 方向键换天、1–6 改张数、Enter 显影、⌘Z 撤销；确认「精选册」页码与「已显影 N 天」同步，并看真实照片下纸张/排版是否还「假」。
4. **GUI 手测导出**：`npm run tauri dev` → 选几张 → 顶栏「导出」→ 选目录 → 预览 → 导出，确认进度条、清单文件 `.wanflow-export.json`、重复运行不产生 `_2` 副本。
5. **补 lint/format**：`npm i -D eslint prettier` + `rustfmt.toml`，让改动有自动检查。
6. **从蓝图 v0.3 里挑一项**：加密库 / `.wanpack` 打包 / 批量处理，建议先做「批量处理」（不涉及加密与协同，风险最低、用户感知最强）。

## 阻塞 / 需要人决定

| 问题 | 我需要你选什么 | 我的建议 |
|---|---|---|
| 蓝图 v0.3 的功能先做哪个 | 加密 / 打包 / 批量 / 协同 / 动效 | 先「批量处理」，其余按用户反馈排 |
| M4 剩余三项（ONNX 质量模型 / 家人注册 / QQ 空间 API）优先级 | 三项里挑一项 | 先补 ONNX 质量模型（纯本地、离线可用） |
| 场景阈值校准缺真实样本 | 指定一个家庭照片目录，或授权我读 Photos 图库导出样本（3–5 天，含同场景多拍） | 先给 3 天即可看出 0.82/90min 是否偏严 |

## 当前环境的坑（换机器/换 agent 必看）

- **用 npm，不用 pnpm**：仓库只有 `package-lock.json`，混用会让 tauri 依赖版本漂移。
- **`npm run tauri build` 的 dmg 步骤会挂死**：`bundle_dmg.sh` 用 osascript 让 Finder 排版窗口，无 GUI 自动化权限时永久卡在「Running bundle_dmg.sh」（本次卡 70s+ 无增长）。此时 `.app` 已生成，直接 `hdiutil create -volname WanFlow -srcfolder src-tauri/target/release/bundle/macos/WanFlow.app -ov -format UDZO <out>.dmg` 出包（18 MiB，无自定义背景排版，可直接发）。
- `src-tauri/target/` 已 15G（编译产物），**永远不要提交、不要打包、不要 `du` 全盘扫**（会很慢）。
- 改 Rust 必须 `npm run tauri dev`，只 `npm run dev` 看不到 Rust 侧改动。
- 原图非破坏：任何「整理」类改动都要先 dry-run 预览，且不得写源文件。导出目录**不能**放在来源目录里（`export.rs` 会拒绝）。
- 前端没有 `vue-tsc`，`tsc --noEmit` 会报 `Cannot find module './App.vue'`（缺 shim，属既有问题，不是新改动引入）；类型检查目前只有构建期 esbuild 剥离。

## 最近三次变更

| 日期 | 做了什么 | 回滚方式 |
|---|---|---|
| 2026-11-07 | `feat`: 恢复家人/人脸/AI 功能线并合并到 v0.2.1（faces/family/chat + FamilyPanel + AI 事件回传）— 3346fd4 / a743b26 | `git revert -m 1 a743b26`（去功能线、留 v0.2.1） |
| 2026-11-09 | `chore`: 发版 v0.2.2（dmg 18.0 MiB / sha256 `f27d34bb…`；GitHub Release Latest；CI 绿）— 7386647 / tag v0.2.2 | 发新版本修，**不删 tag** |
| 2026-11-09 | `fix`: 进度提示 ⏳ 改线框图标 + 版本抬到 0.2.2 — 1a7a1d4 | `git revert 1a7a1d4` |
| 2026-11-09 | `refactor`: 审计整改 Top1-3（去 emoji 图标、颜色归位、动效收敛）— 903a4fd | `git revert 903a4fd` |
| 2026-11-07 | `feat`: 应用内检查更新（⌘K + 欢迎页入口，GitHub Release 比对 + 下载地址复制）— e1de2cb / 2a398ef | `git revert e1de2cb 2a398ef` |
| 2026-11-07 | 同步远端：main ac6b71c → 34e9a95（44 提交），拿到 tag v0.1.0/v0.2.0/v0.2.1；本地 21 处未提交改动 stash 暂存 | 恢复暂存：`git stash pop`；版本回退：`git reset --hard ac6b71c` |
| 2026-11-07 | `chore`: 发版 v0.2.1（版本号跟随安装包；dmg 16.97 MiB，sha256 `c86095f8…`）— c7997b7 / 1ee03e7 / tag v0.2.1 | 发新版本修，**不删 tag** |
| 2026-11-07 | `chore`: 发版 v0.2.0（浅色默认 + 新视觉，dmg 16.97 MiB，sha256 `91320c7d…`）— 80a7bc6 / 03d1788 / tag v0.2.0 | 发新版本修，**不删 tag** |
| 2026-11-06 | `feat`: 接入 ui-ux-pro-max 设计系统（暖纸底 + 中性石墨 + calm teal；投影只回浮层）— 057fe6d / 8dd750b | `git revert 8dd750b 057fe6d` |
| 2026-11-06 | `refactor`: 前端按微件视觉规范去装饰（tokens / styles / Storyline / EmptyState 等 8 文件，+163/−274）— 52ac1c5 | `git revert 52ac1c5` |
