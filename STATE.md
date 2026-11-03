# STATE.md —— 当前状态（每轮开工先读这个）

> 规则：只写"现在的真实情况"，不写历史。历史进 `HANDOFF.md`。
> 每次收工前更新三处：版本、进行中、下一步。旧内容删掉，不要堆积。

**最后更新**：2026-11-04 · 更新人：DSH / lead · 设备：开发机（macOS）

## 一句话现状

v0.1.0（未发版）。**从导入到交付的闭环已打通**：导入（文件夹 / 照片图库 / WebDAV）→ 扫描 → 质量分析 → 去重与连拍分组 → 初筛建议 → 人工决策 → 智能精选 / 语义搜索 → **精选集导出**（dry-run 预览 + 副本导出，支持目录结构、命名模板、剥离 EXIF、改写时间、导出清单幂等）。
规模：Rust `src-tauri/src` 5195 行 / 前端 `src` 2264 行；**32 个 Tauri 命令**；`cargo test` **40 个测试全绿**；`npm run build` 通过。
蓝图 v0.3 里的在线协同（CRDT）、`.wanpack` 整库打包、本地加密库**尚未开始**。

## 进行中

| 事项 | 状态 | 卡在哪 | 谁在做 |
|---|---|---|---|
| — | 无（导出功能已实现、已测、待提交） | — | — |

## 下一步（按先后顺序）

1. **GUI 手测导出**：`npm run tauri dev` → 选几张 → 顶栏「导出」→ 选目录 → 预览 → 导出，确认进度条、清单文件 `.wanflow-export.json`、重复运行不产生 `_2` 副本。
2. **补 lint/format**：`npm i -D eslint prettier` + `rustfmt.toml`，让改动有自动检查。
3. **从蓝图 v0.3 里挑一项**：加密库 / `.wanpack` 打包 / 批量处理，建议先做「批量处理」（不涉及加密与协同，风险最低、用户感知最强）。

## 阻塞 / 需要人决定

| 问题 | 我需要你选什么 | 我的建议 |
|---|---|---|
| 蓝图 v0.3 的功能先做哪个 | 加密 / 打包 / 批量 / 协同 / 动效 | 先「批量处理」，其余按用户反馈排 |
| M4 剩余三项（ONNX 质量模型 / 家人注册 / QQ 空间 API）优先级 | 三项里挑一项 | 先补 ONNX 质量模型（纯本地、离线可用） |

## 当前环境的坑（换机器/换 agent 必看）

- **用 npm，不用 pnpm**：仓库只有 `package-lock.json`，混用会让 tauri 依赖版本漂移。
- `src-tauri/target/` 已 15G（编译产物），**永远不要提交、不要打包、不要 `du` 全盘扫**（会很慢）。
- 改 Rust 必须 `npm run tauri dev`，只 `npm run dev` 看不到 Rust 侧改动。
- 原图非破坏：任何「整理」类改动都要先 dry-run 预览，且不得写源文件。导出目录**不能**放在来源目录里（`export.rs` 会拒绝）。
- 前端没有 `vue-tsc`，`tsc --noEmit` 会报 `Cannot find module './App.vue'`（缺 shim，属既有问题，不是新改动引入）；类型检查目前只有构建期 esbuild 剥离。

## 最近三次变更

| 日期 | 做了什么 | 回滚方式 |
|---|---|---|
| 2026-11-04 | `feat`: 精选集导出（`src-tauri/src/export.rs` + `src/components/ExportModal.vue` + api/App 接线 + 9 个导出测试） | `git revert <本次 sha>` |
| 2026-11-04 | `test`: 给导出补 10 个测试（全库 40 个；此前 STATE 里"0 测试"是过期信息，实际早已有 30 个） | `git revert <sha>` |
| 2026-11-04 | `fix`: `examples/embed_bench.rs` 的 `MutexGuard` 缺 `mut` 导致 `cargo test` 编译失败 | `git revert <sha>` |
