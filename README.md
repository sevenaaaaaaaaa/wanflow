# WanFlow · 家庭照片整理台

面向宝妈宝爸的本地优先照片/视频整理工具：多源导入、自动去重、时间修复、键盘快速挑拣，输出可直接做相册/成片的精选集。**原图永不被修改。**

当前进度：**M3 多源导入**（详见 [BLUEPRINT.md](BLUEPRINT.md)，调研见 [RESEARCH.md](RESEARCH.md)）。

## M3 已有功能（在 M2 之上）

| 功能 | 说明 |
|---|---|
| 导入中心 | 顶栏「导入」打开统一入口：本地文件夹 / macOS 照片图库 / 网盘 WebDAV / QQ·微信导出目录 / 来源管理 |
| macOS 照片图库 | 优先 osxphotos（自动装进 `~/.wanflow/osxphotos-venv`，也可用 PATH 里的）；保留原始文件名 + **图库记录时间直接作为权威拍摄时间**（272/272 全覆盖，HEIC 无 EXIF 也不怕）；iCloud 未下载的自动跳过并可之后重扫。没装 osxphotos 时回退直扫 originals 目录 |
| 网盘 WebDAV | 兼容 Alist/群晖/Apache/nginx dav：递归列目录（防爆炸上限）→ 增量下载到暂存区（同大小+同 mtime 跳过）→ 恢复远端修改时间 → 扫描。**密码只进系统钥匙串**。配合 [Alist](https://github.com/AlistGo/alist) 一个协议覆盖百度/阿里/夸克/OneDrive |
| QQ·微信导出目录 | 导入企鹅相册(QzonePhoto)/QQ空间导出助手等下载的目录，文件名时间戳自动恢复拍摄时间（EXIF 普遍丢失的场景） |
| 来源管理 | 每个来源记录类型/路径/数量/上次同步时间；支持**增量重新同步**（folder/qq 重扫、photos 重查询、webdav 增量下载）和解除关联（文件与决策保留） |
| 溯源 | media 表挂 source_id，跨来源内容 hash 去重依然生效（同照片多来源只算重复组） |

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
cd src-tauri && cargo test   # 28 个测试：pHash/时间恢复/扫描去重/质量指标/连拍/建议/WebDAV解析
```

真实库性能分析（对已导入的库跑一次完整分析，第二连接 WAL 安全）：

```bash
cd src-tauri && cargo run --example analyze_bench
```

## 技术栈

Tauri 2（macOS WKWebView / Windows WebView2，原生无 Chromium 捆绑）· Rust 核心（rayon 并行扫描、rusqlite 旁车库、image 缩略图、自实现 DCT pHash）· Vue 3 + Vite。

## 路线图（详见 BLUEPRINT.md v0.3）

- **M4** CLIP 语义索引 + Ollama 三层 agent + 中文配额选片 + ONNX 模型升级（MUSIQ/人脸/闭眼）+ 家人注册；QQ 空间直连 API（扫码登录）
- **M5** CRDT 家人协同（yrs，走用户自己的 WebDAV/S3）+ `.wanpack` 整库迁移
- **M6** 视频精彩段 + 导出（修正EXIF/去GPS/Immich/pyCapCut 剪映草稿）
- **M7** 本地加密库（SQLCipher + 媒体逐文件 AES-256-GCM）
