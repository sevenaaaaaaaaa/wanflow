// 仅开发期：浏览器里没有 Tauri 运行时，用一份可达的演示库驱动界面。
// 生产构建里 import.meta.env.DEV 为 false，且入口会检查 __TAURI_INTERNALS__，不会生效。
import type { HistoryEntry, MediaItem, Stats } from "../api";
import { api } from "../api";

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function photo(seed: number, w = 640, h = 480): string {
  const hue = (seed * 37) % 360;
  const sun = 18 + ((seed * 29) % 62);
  const horizon = 118 + ((seed * 17) % 46);
  const hill1 = horizon + 6 + ((seed * 7) % 22);
  const svg =
    '<svg xmlns="http://www.w3.org/2000/svg" width="' + w + '" height="' + h + '" viewBox="0 0 320 240">' +
    '<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1">' +
    '<stop offset="0" stop-color="hsl(' + hue + ' 42% 78%)"/><stop offset=".62" stop-color="hsl(' + (hue + 16) + ' 46% 62%)"/>' +
    '<stop offset="1" stop-color="hsl(' + (hue + 26) + ' 38% 44%)"/></linearGradient>' +
    '<radialGradient id="g" cx=".5" cy=".42" r=".75"><stop offset=".5" stop-color="#000" stop-opacity="0"/>' +
    '<stop offset="1" stop-color="#000" stop-opacity=".28"/></radialGradient></defs>' +
    '<rect width="320" height="240" fill="url(#s)"/>' +
    '<circle cx="' + (sun * 4.6) + '" cy="' + (58 + ((seed * 11) % 34)) + '" r="' + (13 + (seed % 9)) + '" fill="hsl(' + (hue + 48) + ' 88% 86%)" opacity=".85"/>' +
    '<path d="M0 ' + hill1 + ' Q 62 ' + (horizon - 30) + ' 132 ' + (hill1 - 8) + ' T 320 ' + (hill1 - 2) + ' V240 H0Z" fill="hsl(' + (hue + 120) + ' 20% 30%)" opacity=".92"/>' +
    '<path d="M0 ' + (hill1 + 34) + ' Q 96 ' + (hill1 + 4) + ' 196 ' + (hill1 + 26) + ' T 320 ' + (horizon + 6) + ' V240 H0Z" fill="hsl(' + (hue + 138) + ' 22% 20%)"/>' +
    '<rect width="320" height="240" fill="url(#g)"/></svg>';
  return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
}

interface DayPlan { ago: number; count: number }
const PLANS: DayPlan[] = [
  { ago: 0, count: 17 }, { ago: 1, count: 28 }, { ago: 2, count: 9 }, { ago: 3, count: 22 },
  { ago: 5, count: 7 }, { ago: 7, count: 14 }, { ago: 10, count: 26 },
  { ago: 14, count: 5 }, { ago: 18, count: 19 }, { ago: 24, count: 12 },
];

let idSeq = 1;
const items: MediaItem[] = [];

for (const plan of PLANS) {
  const base = new Date();
  base.setHours(18, 40, 0, 0);
  base.setDate(base.getDate() - plan.ago);
  let burst: number | null = null;
  let burstLeft = 0;
  for (let i = 0; i < plan.count; i++) {
    const t = Math.floor(base.getTime() / 1000) - i * 90 - (i % 3) * 40;
    if (burstLeft === 0 && i > 2 && (i + plan.ago) % 6 === 0) {
      burst = 500 + idSeq;
      burstLeft = 2 + ((i + plan.ago) % 3);
    } else if (burstLeft > 0) {
      burstLeft--;
    }
    const seed = idSeq * 13 + plan.ago * 7;
    const id = idSeq++;
    const decidedKeep = id % 11 === 0;
    const decidedReject = !decidedKeep && id % 17 === 0;
    items.push({
      id,
      path: "/demo/IMG_" + (2000 + id) + ".HEIC",
      filename: "IMG_" + (2000 + id) + ".HEIC",
      kind: "image",
      sizeBytes: 1800000 + ((id * 7919) % 2600000),
      thumb: photo(seed),
      preview: photo(seed, 1280, 960),
      width: 4032,
      height: 3024,
      dupGroup: id % 23 === 0 ? 700 + (id % 5) : null,
      decision: decidedKeep ? "keep" : decidedReject ? "reject" : null,
      stars: decidedKeep ? 3 + (id % 3) : null,
      effTime: t,
      timeSource: "exif",
      timeConfidence: 0.62 + ((id * 7) % 30) / 100,
      quality: 26 + ((id * 31) % 68),
      sharpness: 20 + ((id * 17) % 70),
      exposure: 30 + ((id * 23) % 60),
      burstGroup: burst,
      burstBest: burst !== null && burstLeft === 0 ? 1 : null,
    });
  }
}

interface Snap { decision: MediaItem["decision"]; stars: number | null }
interface Change { ids: number[]; prev: Snap[]; next: Snap[]; label: string }
const undoStack: Change[] = [];
const redoStack: Change[] = [];

function snapshot(list: MediaItem[]): Snap[] {
  return list.map((m) => ({ decision: m.decision, stars: m.stars }));
}
function affected(ids: number[]): MediaItem[] {
  return items.filter((m) => ids.includes(m.id));
}
function restore(list: MediaItem[], snaps: Snap[]) {
  list.forEach((m, i) => {
    if (!snaps[i]) return;
    m.decision = snaps[i].decision;
    m.stars = snaps[i].stars;
  });
}
const history: HistoryEntry[] = [];

function stats(): Stats {
  const keeps = items.filter((m) => m.decision === "keep").length;
  const rejects = items.filter((m) => m.decision === "reject").length;
  const pendings = items.filter((m) => m.decision === "pending").length;
  return {
    total: items.length,
    images: items.length,
    videos: 0,
    bytes: items.reduce((n, m) => n + m.sizeBytes, 0),
    keeps, rejects, pendings,
    unreviewed: items.length - keeps - rejects - pendings,
    dupGroups: new Set(items.map((m) => m.dupGroup).filter((g) => g !== null)).size,
    lastScan: Math.floor(Date.now() / 1000),
  };
}

function applyDecision(ids: number[], decision: MediaItem["decision"], stars: number | null): string {
  const list = affected(ids);
  const prev = snapshot(list);
  const label = decision
    ? "标记 " + ids.length + " 张为" + (decision === "keep" ? "保留" : decision === "reject" ? "淘汰" : "待定")
    : "设置 " + ids.length + " 张星级";
  history.unshift({ id: history.length + 1, label, kind: "decision", undone: false, createdAt: Math.floor(Date.now() / 1000) });
  for (const m of list) {
    if (decision !== null) m.decision = decision;
    if (stars !== null) m.stars = stars;
  }
  redoStack.length = 0;
  undoStack.push({ ids: list.map((m) => m.id), prev, next: snapshot(list), label });
  return label;
}

export function installDevMock(): void {
  if (import.meta.env.PROD || hasTauri()) return;

  api.listMedia = async (q) => {
    let out = items.filter((m) => {
      if (q.decision === "unreviewed") return m.decision === null;
      if (q.decision) return m.decision === q.decision;
      return true;
    });
    if (q.dupOnly) out = out.filter((m) => m.dupGroup !== null);
    const dir = q.dir === "asc" ? 1 : -1;
    const sort = q.sort ?? "added";
    out = [...out].sort((a, b) => {
      if (sort === "time") return (a.effTime - b.effTime) * dir;
      if (sort === "name") return a.filename.localeCompare(b.filename) * dir;
      if (sort === "size") return (a.sizeBytes - b.sizeBytes) * dir;
      if (sort === "stars") return ((a.stars ?? 0) - (b.stars ?? 0)) * dir;
      if (sort === "quality") return ((a.quality ?? -1) - (b.quality ?? -1)) * dir;
      return (a.id - b.id) * dir;
    });
    const offset = q.offset ?? 0;
    return { items: out.slice(offset, offset + (q.limit ?? 200)), total: out.length };
  };
  api.stats = async () => stats();
  api.getScanStatus = async () => ({ running: false, phase: "", done: 0, total: 0, current: "" });
  api.getSuggestions = async () => [];
  api.analyzeAll = async () => {};
  api.setDecision = async (ids, decision, stars) => applyDecision(ids, decision as MediaItem["decision"], stars);
  api.developPage = async (keepIds, restIds) => {
    const keep = keepIds.filter((id, i) => keepIds.indexOf(id) === i);
    const all = [...keep, ...restIds.filter((id) => !keep.includes(id))];
    const label = restIds.length
      ? "显影 " + keep.length + " 张 · 其余 " + restIds.length + " 张转待定"
      : "显影 " + keep.length + " 张";
    const list = affected(all);
    const prev = snapshot(list);
    for (const m of list) {
      if (keep.includes(m.id)) m.decision = "keep";
      else m.decision = "pending";
    }
    redoStack.length = 0;
    undoStack.push({ ids: list.map((m) => m.id), prev, next: snapshot(list), label });
    return label;
  };
  api.undoLast = async () => {
    const last = undoStack.pop();
    if (!last) return null;
    restore(affected(last.ids), last.prev);
    redoStack.push(last);
    return last.label;
  };
  api.redoLast = async () => {
    const last = redoStack.pop();
    if (!last) return null;
    restore(affected(last.ids), last.next);
    undoStack.push(last);
    return last.label;
  };
  api.history = async () => history;
  api.exportStatus = async () => ({ running: false, phase: "", done: 0, total: 0, current: "" });
  api.listSources = async () => [];
  api.listAlbums = async () => [];
  api.clipStatus = async () => ({ modelsReady: false, embCount: 0, imageCount: 0, ollamaReady: false, ollamaModels: [] });
}
