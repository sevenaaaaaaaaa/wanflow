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

export const api = {
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
  createAlbum: (name: string, ids: number[], scores?: number[]) =>
    invoke<number>("create_album", { name, ids, scores }),
  deleteAlbum: (id: number) => invoke<void>("delete_album", { id }),
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
