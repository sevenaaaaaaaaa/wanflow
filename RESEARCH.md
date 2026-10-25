# WanFlow 调研：AI 选片 / 去废片 / 相册整理 开源项目扫描

> 调研日期：2026-10-24。目标：为孩子照片/视频素材做一个「快速整理 → 挑选 → 输出给相册/视频制作」的本地优先工具，博采众长。

## 一、核心参考项目（按相关度）

### 1. facet — 本地 AI 评分 + 选片引擎（303★，活跃）
https://github.com/ncoevoet/facet

Python + FastAPI + Angular，完全本地，无需 API Key。**功能最全的一个**：

- 9 维评分：美学、构图、人脸质量、眼部清晰度、技术清晰度、色彩、曝光、主体显著性、动态范围；按 30+ 内容类别用不同权重
- **连拍分组**（burst detection）+ 按清晰度/质量/闭眼自动选组内最佳 —— 正对"孩子乱拍一堆"的痛点
- **闭眼检测**（每张脸的眼睛开合、表情、置信度徽章）—— 拍娃刚需
- 相似组聚类（跨时间找相似照片）；**包围曝光/全景序列保护**（不误杀合成素材）
- Auto-Cull：一键按范围（连拍/相似/场景）淘汰，带 **dry-run 预览 + keeper 预算**（"每组只留最好的 1 张"）
- 场景（Scene）：按拍摄时间间隔把一次拍摄切成"场景"，按故事顺序选片
- **Junk Sweep**：零样本识别截图/文件/小票/表情包等非照片垃圾
- 人脸识别 + 聚类管理 + 合并建议；语义搜索（"海边的日落"）；时间线/地图/相册/智能相册
- **偏好学习**：你的评分/淘汰/收藏决策反哺权重优化器（"My Taste" 排序）
- VLM 自然语言点评、AI 描述（GPU 可选）
- 支持 RAW（10 种）、HEIC/HEIF/HIF、AVIF 等，Lightroom 合成 DNG 回退到内嵌预览

**借鉴点**：评分维度体系、连拍+闭眼、dry-run + 预算的自动淘汰、场景切分、垃圾清扫、偏好学习。

### 2. PixCull — 六轴评分 + 可解释理由（123★，活跃）
https://github.com/ChrisChen667788/pixcull

面向"需要向客户解释为什么删"的摄影师。中文界面，macOS Apple Silicon。

- 每张照片 six-axis rubric 评分，分成 保留/待定/淘汰
- **一句话人话理由**（"这张保留了 0.84，因为…"）—— 可解释性是它的灵魂
- 本地优先，云端 judge（MiniMax M3）可选；scores.csv 全量审计
- 借鉴点：**评分必须带理由**；keep/maybe/cull 三档而不是二值；审计文件。

### 3. Norma — 中文 · 美学筛选 + 自然语言选片
https://github.com/HuanYn/Norma

- MUSIQ-AVA（美学）+ MUSIQ-KonIQ10k（技术质量）双模型评分
- **相似只留最佳**：感知哈希 + OpenCLIP + 空间匹配三合一折叠相似构图，并展示"为什么折叠"；原图永不移动
- **自然语言选片带配额**："挑 6 张，1 张人物 5 张风景""挑 4 张雪山"——数量/类别配额/必留照片
- Qwen3-VL-2B 本地解析文字要求；图片相关性用本地 OpenCLIP
- 个性化重排：从你的反馈案例学习偏好
- 借鉴点：**pHash+CLIP+空间校验的去重三保险**；**带配额的自然语言选片**（做相册时直接说"周岁相册要 20 张，10 张特写"）。

### 4. photo-selector（Yan-ShiBo）— 本地三层 Agent + 擂台 UI
https://github.com/Yan-ShiBo/photo-selector

中文，Windows + NVIDIA + Ollama，但架构思想完全可移植：

- **三层链路**：① 基础工具层（EXIF/清晰度/曝光/美学/人脸/闭眼/DINOv2 相似度）→ ② 视觉观察层 qwen3-vl:8b（读图+工具结果，输出观察/瑕疵/亮点）→ ③ 推理决策层 qwen3:14b（初筛建议+组内排序）
- 模型只给建议，人做最终裁决；争议照片进**擂台**（左右对比，←/→ 选择，都留/都弃/稍后再决，AI 还能给出两图差异分析）
- 输出 `winners/` 与 `waste/` 目录；JPG/HEIC 与同名 RAW 自动配对一起移动
- 借鉴点：**工具层→VLM 观察→LLM 决策的 agent 架构**；**擂台式人机协同 UI**；RAW 配对。

### 5. photo-selector-app（harj）— Claude 视觉云评分的桌面应用
https://github.com/harj/photo-selector-app

Electron 桌面应用，明确面向"家庭度假/活动照片"。

- 只把 ~400px 压缩缩略图发给 Claude API 评分（原图不出本机），约 $0.01–0.02/张
- 相似分组每组只显示最佳；按分数过滤批量选；**自定义评估标准 prompt**（"我更在意孩子的表情而不是构图"）
- 导出选中项为高质量 JPG 顺序命名
- 借鉴点：**缩略图上云的成本/隐私模型**；**用户可自定义评分标准**——家用场景很重要（娃的表情 > 摄影技法）。

### 6. Immich / PhotoPrism / LibrePhotos — 管理平台层（不做轮子，做对接）
- Immich：自托管 Google Photos 替代，CLIP 语义搜索、人脸识别聚类、时间线。生态有 MCP：[drolosoft/immich-photo-manager](https://github.com/drolosoft/immich-photo-manager)（59★，CLIP/OCR 搜索、地理相册、重复检测）、[BarryW/ImmichMCP](https://github.com/BarryW/ImmichMCP)（55★）
- [sweetrb/apple-photos-mcp](https://github.com/sweetrb/apple-photos-mcp)（33★）：让 Claude 直接查询/导出 macOS 照片图库
- 借鉴点：WanFlow 不必重建浏览/备份平台，**精选结果可直接推给 Immich 或导出文件夹**；也可给 WanFlow 自己做一个 MCP 接口让 AI 助手操作图库。

## 二、去重/废片专项

| 项目 | 方式 | 备注 |
|---|---|---|
| [czkawka](https://github.com/qarmin/czkawka)（Rust） | 感知哈希找相似图，**还能找相似视频** | 速度快，GUI+CLI；相似视频思路值得抄 |
| [imagededup](https://github.com/idealo/imagededup) | CNN + 多种哈希（PHash/DHash/AHash/WHash） | Python 库，可直接 pip 依赖 |
| dupeGuru Picture 模式 | 模糊匹配 | 老牌参考 |
| CLIP 语义去重 | embedding 余弦 > ~0.95 判近重复（[CLIP#260](https://github.com/openai/CLIP/issues/260)、[HF dedup 工具](https://github.com/huggingface/large-scale-image-deduplication) | 哈希抓不到的"语义相同但裁剪/滤镜不同" |
| videohash | 视频感知哈希 | 视频去重候选方案 |

## 三、下游（整理完做相册/视频）

- [pyCapCut](https://github.com/GuanYixuan/pyCapCut)（965★）：Python 生成剪映草稿，可自动化成片 —— 精选照片直接喂给它出成长视频
- 本环境已有 video-agent-kit 插件（自动剪辑/串片/配乐工作流），可作为 WanFlow 的"成片"后端

## 三点五、导入源与元数据（第二轮补充调研）

### QQ 空间相册下载（历史照片金矿——宝妈宝爸 2010–2018 年的照片大多在 QQ 空间）
- [QzoneArchive](https://github.com/Gaoshu705/QzoneArchive)（1335★，活跃）：QQ 空间动态/照片/视频/互动记录归档到本地，桌面+移动端
- [QzonePhoto 企鹅相册](https://github.com/11273/QzonePhoto)（207★，活跃）：**用官方 API**，扫码登录，批量下载/上传、断点续传、智能分类、隐私保护模式，原图原画
- [QZoneExport](https://github.com/ShunCai/QZoneExport)：浏览器扩展，三步备份相册/说说/日志/留言板
- [qzone-photo-downloader](https://github.com/dslwind/qzone-photo-downloader)：Python，CLI+GUI，断点续传、多线程、好友相册
- [QQGroupAlbumDownload](https://github.com/lihengdao666/QQGroupAlbumDownload)：QQ **群**相册批量下载（班级群、亲友群的娃照片大多在这）
- 借鉴点：QQ 下载的照片**普遍丢失 EXIF**，拍摄时间只能从相册结构/文件名/接口字段恢复 —— 正好是"时间修正"层的用武之地

### 网盘（一个入口全覆盖）
- [Alist](https://github.com/AlistGo/alist)（50351★，活跃）：聚合百度网盘/阿里云盘/夸克/天翼/ OneDrive/Google Drive 等，统一 WebDAV + API。**WanFlow 不用逐个对接网盘 SDK，接 WebDAV 一招通吃**，让用户自装 Alist 或内嵌引导
- 阿里云盘单独方案：[aliyundrive-webdav](https://github.com/messense/aliyundrive-webdav)（9734★）

### 系统相册
- macOS：[osxphotos](https://github.com/RhetTbull/osxphotos)（3934★，活跃）—— Apple Photos 图库导出+全套元数据，可提取"已编辑版本/Live Photo 配对/相册归属/人物"，是 mac 端导入的成熟轮子（Rust 里可子进程调用或参考其 osxphotosdb 逻辑）
- iPhone 直连：[libimobiledevice](https://github.com/libimobiledevice/libimobiledevice)（8285★）；Android：MTP 协议（后置）

### 元数据污染问题（多设备家庭的核心痛点）
家庭素材源的元数据现状：
- 微信/QQ 保存的图：EXIF 全丢，文件名是 `mmexport1696…`（毫秒时间戳）或 `IMG_xxxx`
- QQ 空间下载：EXIF 全丢，只有相册名+上传时间可参考
- 相机时区没调：DateTimeOriginal 差 8 小时；手机/相机/老人机时间没校准
- 同一张照片在 手机A、手机B、网盘、QQ空间 各有一份，hash 不同（压缩/转码）、时间不同、名字不同
业界方案（facet/Yan-ShiBo/Norma 共同做法 + 补充）：
1. **绝不写原图**：一切修正存旁车（sidecar DB）；导出副本时才按需写入修正后的 EXIF
2. 时间恢复级联：EXIF → QuickTime mvhd/视频元数据 → 文件名模式（IMG_20231005_123456 / mmexport 时间戳 / WX_ 日期 / Screenshot_日期）→ 下载器带回的接口时间 → 文件 mtime 兜底
3. 跨设备同源识别：pHash + CLIP embedding + 时间窗，而不是仅文件 hash —— "同一时刻同画面"即视为同一张，合并为单一逻辑照片多来源副本
4. 时区修正：按批次（一次导入/一个设备）整体偏移，参照 GPS 或用户指认

## 四、共性与差异总结

所有好项目共同遵守的原则：
1. **本地优先、非破坏性**：分析归分析，原图永远不动，决策落在旁车数据库/导出副本
2. **分组而非单张**：连拍/相似先聚组，组内留最佳，人只看组代表
3. **可解释**：每个淘汰决定给理由，人可一键召回
4. **dry-run**：自动淘汰先预览再执行
5. **人机分工**：AI 处理"明显废片+排序"，人只裁决擂台上的疑难杂症

家用拍娃场景的差异化需求（现有项目覆盖不足，即 WanFlow 的机会）：
- **闭眼/表情优先级远高于构图**：影楼工具按摄影美学加权，家里应按"表情>清晰>构图"
- **指定人物加权**：只要"我家娃"在照片里且状态好就加分，路人/背景降权 —— 需要娃的人脸聚类作为一等公民
- **视频素材**：连拍照片有人做，家庭视频的"稳定段/精彩瞬间/晃动废段"几乎没人做
- **成长时间线语义**：按月龄/事件（第一次走路、生日）组织，而不是影楼的"一次拍摄一个 shoot"
- **中文自然语言选片**（Norma 已验证可行）
- **直接对接下游成片**（pyCapCut / video-agent-kit）
