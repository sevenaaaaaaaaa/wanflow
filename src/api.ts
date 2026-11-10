import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";

export interface MediaItem {
  id: number;
  path: string;
  filename: string;
  kind: "image" | "video";
  sizeBytes: number;
  thumb: string | null;
  preview: string | null;
  width: number | null;
  height: number | null;
  dupGroup: number | null;
  decision: "keep" | "reject" | "pending" | null;
  stars: number | null;
  effTime: number;
  timeSource: string;
  timeConfidence: number;
  quality: number | null;
  sharpness: number | null;
  exposure: number | null;
  burstGroup: number | null;
  burstBest: number | null;
  /** 场景键（场景首张照片的 id）；还没建场景索引时为 null */
  sceneGroup: number | null;
}

export interface ListResult {
  items: MediaItem[];
  total: number;
}

export interface RebuildScenesStats {
  scenes: number;
  images: number;
  withEmb: number;
  missingEmb: number;
  changed: number;
}

export interface ScanStatus {
  running: boolean;
  phase: string;
  done: number;
  total: number;
  current: string;
}

export interface ScanSummary {
  inserted: number;
  updated: number;
  unchanged: number;
  skipped: number;
  images: number;
  videos: number;
  dupGroups: number | null;
}

export interface Stats {
  total: number;
  images: number;
  videos: number;
  bytes: number;
  keeps: number;
  rejects: number;
  pendings: number;
  unreviewed: number;
  dupGroups: number;
  lastScan: number;
}

export interface HistoryEntry {
  id: number;
  label: string;
  kind: string;
  undone: boolean;
  createdAt: number;
}

export interface TimeDiffRow {
  id: number;
  filename: string;
  oldTime: number;
  oldSource: string;
  newTime: number | null;
  changed: boolean;
  note: string | null;
}

export interface BatchTimeSpec {
  ids: number[];
  mode: "offset" | "reparse" | "anchor";
  hours?: number;
  minutes?: number;
  anchorMedia?: number;
  anchorNewTime?: number;
}

export interface AnalyzeSummary {
  scored: number;
  bursts: number;
  suggestions: number;
}

export type SuggestionRule = "blur" | "exposure" | "burst";

export interface SuggestionItem {
  id: number;
  mediaId: number;
  rule: SuggestionRule;
  reason: string;
  proposed: string;
  filename: string;
  thumb: string | null;
  quality: number | null;
}

export interface WebdavConfig {
  url: string;
  username: string;
  password: string;
  remotePath: string;
}

export interface AiStatus {
  modelsReady: boolean;
  embCount: number;
  imageCount: number;
  ollamaReady: boolean;
  ollamaModels: string[];
}

export interface ScoredMedia {
  score: number;
  id: number;
  path: string;
  filename: string;
  kind: "image" | "video";
  sizeBytes: number;
  thumb: string | null;
  preview: string | null;
  width: number | null;
  height: number | null;
  dupGroup: number | null;
  decision: "keep" | "reject" | "pending" | null;
  stars: number | null;
  effTime: number;
  timeSource: string;
  timeConfidence: number;
  quality: number | null;
  sharpness: number | null;
  exposure: number | null;
  burstGroup: number | null;
  burstBest: number | null;
  /** 场景键（场景首张照片的 id）；还没建场景索引时为 null */
  sceneGroup: number | null;
}

export interface SmartSelectResult {
  count: number;
  parsedCount: number;
  parsedText: string;
  items: ScoredMedia[];
}

export interface AlbumItem {
  id: number;
  name: string;
  count: number;
  createdAt: number;
}

export interface TagItem {
  id: number;
  name: string;
  count: number;
}

export interface ExportOptions {
  destDir: string;
  scope: "album" | "keeps" | "selection";
  albumId?: number;
  ids?: number[];
  layout: "flat" | "year" | "month" | "album";
  nameTemplate: string;
  stripMetadata: boolean;
  rewriteTime: boolean;
  includeVideos: boolean;
}

export interface ExportRow {
  id: number;
  src: string;
  dest: string;
  filename: string;
  kind: string;
  sizeBytes: number;
  action: "copy" | "skip";
  reason: string | null;
}

export interface ExportPlan {
  rows: ExportRow[];
  copyCount: number;
  skipCount: number;
  copyBytes: number;
  destDir: string;
  notes: string[];
}

export interface ExportSummary {
  copied: number;
  skipped: number;
  failed: number;
  bytes: number;
  destDir: string;
  durationMs: number;
}

export interface FamilyIndexSummary {
  photos: number;
  faces: number;
  clusters: number;
  autoAssigned: number;
}

export interface ClusterItem {
  cluster: number;
  faceCount: number;
  photoCount: number;
  repThumb: string | null;
  personId: number | null;
  personName: string | null;
  isFamily: boolean;
}

export interface FamilyMember {
  id: number;
  name: string;
  faceCount: number;
  photoCount: number;
}

export interface AgentSelectResult {
  items: ScoredMedia[];
  reason: string;
  model: string;
  candidates: number;
}

export interface SourceItem {
  id: number;
  kind: "folder" | "photos" | "webdav" | "qq";
  label: string;
  rootPath: string | null;
  url: string | null;
  lastScanned: number | null;
  mediaCount: number;
}

export interface UpdateCheckResult {
  current: string;
  latest: string | null;
  newer: boolean;
  url: string | null;
  error: string | null;
}

/** 语义化版本比较：>0 表示 a 更新 */
function cmpSemver(a: string, b: string): number {
  const pa = a.replace(/^v/, "").split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.replace(/^v/, "").split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}


// —— .wanpack 整库打包 ——

export interface WanpackStats {
  media: number;
  actions: number;
  tags: number;
  albums: number;
  faces: number;
}

export interface WanpackSource {
  id: number;
  kind: string;
  label: string;
  root: string | null;
}

export interface WanpackFile {
  path: string;
  bytes: number;
  sha256: string;
}

export interface WanpackManifest {
  wanpackVersion: number;
  appVersion: string;
  schemaVersion: number;
  createdAt: number;
  stats: WanpackStats;
  sources: WanpackSource[];
  files: WanpackFile[];
}

export interface WanpackPlan {
  dest: string;
  dbBytes: number;
  stats: WanpackStats;
  sources: WanpackSource[];
  blocked: string[];
}

export interface WanpackOutcome {
  dest: string;
  packBytes: number;
  dbBytes: number;
  stats: WanpackStats;
  sha256: string;
}

export interface WanpackRestoreOutcome {
  dest: string;
  stats: WanpackStats;
  mediaRewritten: number;
  sourcesRewritten: number;
  mediaUnmapped: number;
}

export interface WanpackRemap {
  from: string;
  to: string;
}

export const api = {
  wanpackPlan: (dest: string) => invoke<WanpackPlan>("wanpack_plan", { dest }),
  wanpackPack: (dest: string, dryRun = false, overwrite = false) =>
    invoke<WanpackOutcome>("wanpack_pack", { dest, dryRun, overwrite }),
  wanpackInspect: (pack: string) => invoke<WanpackManifest>("wanpack_inspect", { pack }),
  wanpackRestore: (pack: string, dest: string, remap: WanpackRemap[], dryRun = false, overwrite = false) =>
    invoke<WanpackRestoreOutcome>("wanpack_restore", { pack, dest, remap, dryRun, overwrite }),
  importFolder: (path: string, kind: "folder" | "qq", label: string) =>
    invoke<void>("import_folder", { path, kind, label }),
  importPhotosLibrary: () => invoke<void>("import_photos_library"),
  importWebdav: (config: WebdavConfig) => invoke<void>("import_webdav", { config }),
  rescanSource: (id: number) => invoke<void>("rescan_source", { id }),
  unlinkSource: (id: number) => invoke<void>("unlink_source", { id }),
  listSources: () => invoke<SourceItem[]>("list_sources"),
  getScanStatus: () => invoke<ScanStatus>("get_scan_status"),
  listMedia: (query: {
    decision?: string;
    sort?: string;
    dir?: string;
    dupOnly?: boolean;
    tagId?: number;
    offset?: number;
    limit?: number;
  }) => invoke<ListResult>("list_media", { query }),
  stats: () => invoke<Stats>("stats"),
  rebuildScenes: () => invoke<RebuildScenesStats>("rebuild_scenes"),
  setDecision: (ids: number[], decision: string | null, stars: number | null) =>
    invoke<string>("set_decision", { ids, decision, stars }),
  developPage: (keepIds: number[], restIds: number[]) =>
    invoke<string>("develop_page", { keepIds, restIds }),
  undoLast: () => invoke<string | null>("undo_last"),
  redoLast: () => invoke<string | null>("redo_last"),
  history: (limit?: number) => invoke<HistoryEntry[]>("history", { limit }),
  batchTimePreview: (spec: BatchTimeSpec) => invoke<TimeDiffRow[]>("batch_time_preview", { spec }),
  batchTimeApply: (spec: BatchTimeSpec) => invoke<string>("batch_time_apply", { spec }),
  dupMembers: (group: number) => invoke<MediaItem[]>("dup_members", { group }),
  burstMembers: (group: number) => invoke<MediaItem[]>("burst_members", { group }),
  analyzeAll: () => invoke<void>("analyze_all"),
  getSuggestions: () => invoke<SuggestionItem[]>("get_suggestions"),
  clipStatus: () => invoke<AiStatus>("clip_status"),
  clipDownload: () => invoke<void>("clip_download"),
  embedAll: () => invoke<void>("embed_all"),
  semanticSearch: (query: string, limit?: number) =>
    invoke<ScoredMedia[]>("semantic_search", { query, limit }),
  smartSelect: (prompt: string, pool?: string) =>
    invoke<SmartSelectResult>("smart_select", { query: { prompt, pool } }),
  listAlbums: () => invoke<AlbumItem[]>("list_albums"),
  listTags: () => invoke<TagItem[]>("tag_list"),
  assignTags: (ids: number[], name: string, remove = false) =>
    invoke<string>("tag_assign", { ids, name, remove }),
  createAlbum: (name: string, ids: number[], scores?: number[]) =>
    invoke<number>("create_album", { name, ids, scores }),
  deleteAlbum: (id: number) => invoke<void>("delete_album", { id }),
  addToAlbum: (albumId: number, ids: number[]) =>
    invoke<number>("add_to_album", { albumId, ids }),
  removeFromAlbum: (albumId: number, ids: number[]) =>
    invoke<number>("remove_from_album", { albumId, ids }),
  albumMedia: (id: number) => invoke<ScoredMedia[]>("album_media", { id }),
  familyIndexAll: () => invoke<void>("family_index_all"),
  familyClusters: () => invoke<ClusterItem[]>("family_clusters"),
  familyMembers: () => invoke<FamilyMember[]>("family_members"),
  familyRegister: (cluster: number, name: string) =>
    invoke<number>("family_register", { cluster, name }),
  familyAssignCluster: (cluster: number, personId: number) =>
    invoke<void>("family_assign_cluster", { cluster, personId }),
  familyRemove: (personId: number) => invoke<void>("family_remove", { personId }),
  agentSelect: (prompt: string, pool?: string) =>
    invoke<AgentSelectResult>("agent_select", { query: { prompt, pool } }),
  resolveSuggestions: (ids: number[], apply: boolean) =>
    invoke<string>("resolve_suggestions", { ids, apply }),
  exportPreview: (opts: ExportOptions) => invoke<ExportPlan>("export_preview", { opts }),
  exportApply: (opts: ExportOptions) => invoke<void>("export_apply", { opts }),
  exportStatus: () => invoke<ScanStatus>("export_status"),
  // 检查更新：匿名调 GitHub Releases API（仓库公开；限流/断网时给出原因）
  checkUpdate: async (): Promise<UpdateCheckResult> => {
    const current = __APP_VERSION__;
    try {
      const res = await fetch(
        "https://api.github.com/repos/sevenaaaaaaaaa/wanflow/releases/latest",
        { headers: { Accept: "application/vnd.github+json" } },
      );
      if (!res.ok) {
        return { current, latest: null, newer: false, url: null, error: `GitHub ${res.status}（可能限流，稍后再试）` };
      }
      const rel = (await res.json()) as {
        tag_name?: string;
        html_url?: string;
        assets?: { name: string; browser_download_url: string }[];
      };
      const latest = (rel.tag_name ?? "").replace(/^v/, "") || null;
      const url =
        rel.assets?.find((a) => a.name.endsWith(".dmg"))?.browser_download_url ??
        rel.html_url ??
        null;
      return { current, latest, newer: !!latest && cmpSemver(latest, current) > 0, url, error: null };
    } catch (e) {
      return { current, latest: null, newer: false, url: null, error: e instanceof Error ? e.message : String(e) };
    }
  },
};

function readable(path: string | null): string {
  if (!path) return "";
  // 演示数据 / 远程图直接可用；Tauri 本地路径交给 asset 协议。
  if (path.startsWith("data:") || path.startsWith("http:") || path.startsWith("https:")) return path;
  return convertFileSrc(path);
}

export function thumbUrl(m: MediaItem): string {
  return readable(m.thumb);
}

export function previewUrl(m: MediaItem): string {
  return readable(m.preview || m.thumb);
}

export function assetUrl(path: string): string {
  return convertFileSrc(path);
}

export function fmtTime(secs: number): string {
  return new Date(secs * 1000).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function fmtBytes(b: number): string {
  if (b > 1 << 30) return (b / (1 << 30)).toFixed(1) + " GB";
  if (b > 1 << 20) return (b / (1 << 20)).toFixed(1) + " MB";
  return (b / 1024).toFixed(0) + " KB";
}
