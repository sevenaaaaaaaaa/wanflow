# WanFlow · 家庭照片整理台

面向宝妈宝爸的本地优先照片/视频整理工具：多源导入、自动去重、时间修复、键盘快速挑拣，输出可直接做相册/成片的精选集。**原图永不被修改。**

当前进度：**M5 家人与 agent**（详见 [BLUEPRINT.md](BLUEPRINT.md)，调研见 [RESEARCH.md](RESEARCH.md)）。

## M5 已有功能（在 M4 之上）

| 功能 | 说明 |
|---|---|
| 人脸检测/识别 | YuNet 检测（0.2MB）+ SFace 128 维识别（38MB），OpenCV Zoo 模型，全本地。ArcFace 五点对齐，检测+识别+对齐真实库 4206 张 48 秒（~12ms/张） |
| 家人注册 | 全库扫描 → 嵌入聚类（贪心代表匹配）→ 按出现次数排序的候选簇列表 → 「这是家人」起名注册；新脸自动认领（重扫时与已注册家人比对挂靠）；支持归入/取消 |
| 智能选片家人加权 | 「智能精选」里出现注册家人的照片 +0.08（首人）/ +0.02（后续），家用优先级高于画面质量 |
| agent 对话选片 | 顶栏「智能精选 → 🤖 对话选片」：Ollama 本地 LLM（qwen3 已验证）在 CLIP top-40 候选上重排，只能从候选 id 中选（防幻觉）+ 一句话理由，2.9 秒出结果 |

## M4 已有功能

| 功能 | 说明 |
|---|---|
| CLIP 语义索引 | ViT-B/32 fp32 全本地（29ms/张），模型懒下载双源回退 |
| 语义搜索 | 一句话搜照片，相似度排序 |
| 智能精选 | 自然语言配额（"挑 12 张…"）+ 质量加成 + 排除已淘汰 |
| 精选集 | 保存/查看/删除，为导出备弹药 |
| 模型管理 | 状态条引导，任务互斥 |

## M3 已有功能

| 功能 | 说明 |
|---|---|
| 导入中心 | 顶栏「导入」统一入口：本地文件夹 / macOS 照片图库 / 网盘 WebDAV / QQ·微信导出目录 / 来源管理 |
| macOS 照片图库 | 优先 osxphotos（专用 venv 自动检测）；原始文件名 + 图库记录时间=权威拍摄时间；iCloud 未下载自动跳过；无 osxphotos 回退直扫 originals |
| 网盘 WebDAV | 兼容 Alist/群晖等：递归列目录、增量下载、保留远端 mtime、密码只进钥匙串 |
| QQ·微信导出目录 | 企鹅相册/QQ空间导出助手下载的目录，文件名时间戳自动恢复拍摄时间 |
| 来源管理 | 增量重新同步 / 解除关联（文件与决策保留） |

## M2 已有功能

| 功能 | 说明 |
|---|---|
| 质量评分 | 纯 CV 四维指标：清晰度（Laplacian 方差）/ 曝光（裁剪+均值）/ 对比度 / 色彩丰富度，综合分 0-100。真实库实测 ~24ms/张（debug 构建）。MUSIQ/人脸 ONNX 模型后续接入同一组列 |
| 连拍分组 | 时间紧邻（≤5s）+ 画面相似（pHash ≤12）链式成组，自动选出最佳；只信任 EXIF/容器/文件名/图库时间来源 |
| 智能初筛 | 规则建议（人话理由）：严重欠曝/过曝 → 模糊 → 连拍非最佳。**只建议不删除**：应用才写决策（可整体撤销），忽略则永久静默 |
| 擂台 | 连拍组两两对比：←/→ 选边、B 都留、S 跳过，附指标条和一句话差异分析 |
| UI 集成 | 网格徽章（最佳/连拍/质量分色块）、质量最差优先排序、灯箱三维分数 |

## M1 已有功能

| 功能 | 说明 |
|---|---|
| 文件夹导入 | SHA256 精确去重 + pHash 感知去重（连拍/压缩图/跨设备同源），增量扫描（未变化的文件秒级跳过） |
| 时间恢复级联 | EXIF → 视频容器(mvhd) → 文件名模式（`IMG_20231005_123456` / `mmexport1696…` / `Screenshot_2023-10-05-…` / 微信 `wx_camera_…`）→ 文件修改时间，每张标注来源与置信度 |
| 手动挑拣 | 虚拟网格（万张流畅）、灯箱预览（视频内联播放）、键盘流：`P` 保留 `X` 淘汰 `U` 待定 `0-5` 星级 `←→` 导航 `⌘Z/⇧⌘Z` 撤销重做 |
| 重复组面板 | 点"重复"徽章查看组内成员，一键"只留这张" |
| 批量操作 | 框选批量决策/星级；批量时间修正（整体偏移 / 从文件名重解析 / 锚点对齐），**先 dry-run 预览 diff 再执行** |
| 全量撤销 | 一切决策与时间修正都是事件（actions/verdicts/time_overrides），任意步可撤销可重做，历史抽屉可视 |
| 非破坏 | 所有结果存旁车 SQLite（`~/Library/Caches/com.wanflow.app/wanflow.db`），原图一个字节不动 |

## 运行

```bash
# 依赖：Rust 1.75+、Node 18+
npm install
npx tauri dev     # 开发
npx tauri build   # 打包 .app / .dmg（Windows 同命令出 .msi/.exe）
```

核心逻辑测试：

```bash
cd src-tauri && cargo test   # 31 个测试：pHash/时间恢复/扫描去重/质量指标/连拍/建议/WebDAV/CLIP/人脸对齐
```

真实库性能 / 验证（第二连接 WAL 安全）：

```bash
cd src-tauri && cargo run --release --example analyze_bench   # 质量分析
cd src-tauri && cargo run --release --example embed_bench     # CLIP 语义索引
cd src-tauri && cargo run --release --example family_bench    # 人脸索引 + 聚类
cd src-tauri && cargo run --release --example clip_e2e        # CLIP 语义判别断言
cd src-tauri && cargo run --release --example face_e2e        # 人脸检出 + 同人聚类断言
cd src-tauri && cargo run --release --example agent_e2e       # Ollama 对话选片断言（需 ollama serve）
```

## 技术栈

Tauri 2（macOS WKWebView / Windows WebView2，原生无 Chromium 捆绑）· Rust 核心（rayon 并行扫描、rusqlite 旁车库、image 缩略图、自实现 DCT pHash）· Vue 3 + Vite。

## 路线图（详见 BLUEPRINT.md v0.3）

- **M5** 人脸/闭眼 ONNX + 家人注册；agent 对话选片（Ollama 三层架构已检测就绪）；QQ 空间直连 API
- **M6** CRDT 家人协同（yrs）+ `.wanpack` 整库迁移；精选集导出（修正EXIF/去GPS/剪映草稿）
- **M7** 视频精彩段；本地加密库（SQLCipher + 媒体逐文件 AES-256-GCM）
