// 仅开发期：浏览器里没有 Tauri 运行时，用一份可达的演示库驱动界面。
// 生产构建里 import.meta.env.DEV 为 false，且入口会检查 __TAURI_INTERNALS__，不会生效。
import type { HistoryEntry, MediaItem, Stats } from "../api";
import { api } from "../api";

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 确定性伪随机：同一张演示图每次渲染都一样 */
function seeded(seed: number): () => number {
  let t = seed >>> 0;
  return () => {
    t += 0x6d2b79f5;
    let r = Math.imul(t ^ (t >>> 15), 1 | t);
    r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
    return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
  };
}

/** 演示缩略图：暮色山脊 / 夜色散景 / 窗光 / 海面薄雾 / 林间散景。
 *  刻意避开「发光大球 + 高饱和渐变」的插画感，用低饱和、空气透视和颗粒靠近真实照片。 */
function photo(seed: number, w = 640, h = 480): string {
  const rand = seeded(seed);
  const scene = seed % 5;
  const variant = Math.floor(seed / 5) % 2;
  const defs: string[] = [];
  const parts: string[] = [];
  const r2 = (v: number) => Math.round(v * 100) / 100;
  let dark = false;

  defs.push(
    '<radialGradient id="v" cx=".5" cy=".46" r=".74">' +
      '<stop offset=".55" stop-color="#000" stop-opacity="0"/>' +
      '<stop offset="1" stop-color="#000" stop-opacity=".36"/></radialGradient>',
  );

  if (scene === 0) {
    // 暮色山脊：低饱和天空 + 空气透视
    const sunX = 70 + rand() * 170;
    const hor = 130 + rand() * 18;
    const sky = variant
      ? ['#7d8f9d', '#b7b3a6', '#ecd2b4']
      : ['#66798c', '#a9a69c', '#e2c5a5'];
    defs.push(
      `<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${sky[0]}"/>` +
        `<stop offset=".52" stop-color="${sky[1]}"/><stop offset=".92" stop-color="${sky[2]}"/></linearGradient>`,
    );
    defs.push(
      '<radialGradient id="sun"><stop offset="0" stop-color="#ffe7c2" stop-opacity=".9"/>' +
        '<stop offset=".34" stop-color="#ffd7a6" stop-opacity=".38"/>' +
        '<stop offset="1" stop-color="#ffd7a6" stop-opacity="0"/></radialGradient>',
    );
    parts.push('<rect width="320" height="240" fill="url(#sky)"/>');
    parts.push(`<circle cx="${r2(sunX)}" cy="${r2(hor - 28)}" r="${r2(30 + rand() * 12)}" fill="url(#sun)"/>`);
    parts.push(`<circle cx="${r2(sunX)}" cy="${r2(hor - 28)}" r="${r2(3.6 + rand() * 1.6)}" fill="#fff3d8" opacity=".9"/>`);
    parts.push(`<path d="M0 ${r2(hor)} Q 70 ${r2(hor - 18)} 140 ${r2(hor - 4)} T 320 ${r2(hor - 10)} V240 H0Z" fill="#80878a" opacity=".42"/>`);
    parts.push(`<path d="M0 ${r2(hor + 14)} Q 90 ${r2(hor - 2)} 180 ${r2(hor + 18)} T 320 ${r2(hor + 6)} V240 H0Z" fill="#4c5350" opacity=".78"/>`);
    parts.push(`<path d="M0 ${r2(hor + 42)} Q 80 ${r2(hor + 20)} 170 ${r2(hor + 38)} T 320 ${r2(hor + 28)} V240 H0Z" fill="#2b312d"/>`);
    parts.push(`<rect x="0" y="${r2(hor - 16)}" width="320" height="34" fill="${sky[2]}" opacity=".12"/>`);
  } else if (scene === 1) {
    // 夜色散景：失焦的城市灯光
    dark = true;
    defs.push('<linearGradient id="bg" x1="0" y1="0" x2=".4" y2="1"><stop offset="0" stop-color="#15222c"/><stop offset="1" stop-color="#050a0d"/></linearGradient>');
    const cols = variant ? ['#f2b56e', '#d6c7a8', '#9fc4cf'] : ['#8ec3d6', '#e6dfcd', '#e8a469'];
    cols.forEach((c, i) => {
      defs.push(`<radialGradient id="k${i}"><stop offset="0" stop-color="${c}" stop-opacity=".95"/><stop offset=".5" stop-color="${c}" stop-opacity=".38"/><stop offset="1" stop-color="${c}" stop-opacity="0"/></radialGradient>`);
    });
    parts.push('<rect width="320" height="240" fill="url(#bg)"/>');
    for (let i = 0; i < 18; i++) {
      const x = 10 + rand() * 300;
      const y = 20 + rand() * 200;
      const rad = 2 + rand() * rand() * 15;
      parts.push(`<circle cx="${r2(x)}" cy="${r2(y)}" r="${r2(rad)}" fill="url(#k${i % 3})" opacity="${r2(.22 + rand() * .5)}"/>`);
    }
    parts.push('<rect x="0" y="176" width="320" height="64" fill="#04080b" opacity=".5"/>');
    parts.push(`<circle cx="${r2(36 + rand() * 248)}" cy="${r2(58 + rand() * 90)}" r="1.6" fill="#fff8e8" opacity=".9"/>`);
  } else if (scene === 2) {
    // 窗边逆光：暖墙 + 窗棂 + 失焦绿植，像室内随手拍
    const warm = variant ? ['#e8dfd1', '#b6a795'] : ['#f0e6d7', '#c2b09c'];
    defs.push(`<linearGradient id="bg" x1="0" y1="0" x2=".2" y2="1"><stop offset="0" stop-color="${warm[0]}"/><stop offset="1" stop-color="${warm[1]}"/></linearGradient>`);
    defs.push('<linearGradient id="win" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fffaf0"/><stop offset=".7" stop-color="#f7e9d2"/><stop offset="1" stop-color="#e9d4b6"/></linearGradient>');
    defs.push('<radialGradient id="lt"><stop offset="0" stop-color="#fff8ea" stop-opacity=".6"/><stop offset=".6" stop-color="#fff0d8" stop-opacity=".2"/><stop offset="1" stop-color="#fff0d8" stop-opacity="0"/></radialGradient>');
    defs.push('<radialGradient id="leaf"><stop offset="0" stop-color="#1f2b1c" stop-opacity=".52"/><stop offset=".6" stop-color="#1f2b1c" stop-opacity=".28"/><stop offset="1" stop-color="#1f2b1c" stop-opacity="0"/></radialGradient>');
    parts.push('<rect width="320" height="240" fill="url(#bg)"/>');
    const wx = 168 + rand() * 26;
    parts.push(`<rect x="${r2(wx)}" y="0" width="152" height="202" fill="url(#win)" opacity=".92"/>`);
    parts.push(`<rect x="${r2(wx)}" y="0" width="152" height="202" fill="url(#lt)"/>`);
    parts.push(`<rect x="${r2(wx)}" y="0" width="3.4" height="202" fill="#6d5a45" opacity=".16"/>`);
    parts.push(`<rect x="${r2(wx + 74)}" y="0" width="2.6" height="202" fill="#6d5a45" opacity=".12"/>`);
    parts.push(`<rect x="${r2(wx)}" y="96" width="152" height="2.6" fill="#6d5a45" opacity=".10"/>`);
    parts.push(`<rect x="${r2(wx - 16)}" y="198" width="176" height="6" fill="#7a6550" opacity=".18"/>`);
    parts.push(`<ellipse cx="${r2(wx - 60)}" cy="${r2(96 + rand() * 30)}" rx="110" ry="86" fill="url(#lt)" opacity=".55"/>`);
    for (let i = 0; i < 3; i++) {
      const x = 12 + i * 34 + rand() * 14;
      parts.push(`<rect x="${r2(x)}" y="0" width="${r2(10 + rand() * 14)}" height="210" fill="#5b4a3a" opacity="${r2(.03 + rand() * .03)}"/>`);
    }
    const baseX = 14 + rand() * 24;
    parts.push(`<path d="M${r2(baseX)} 238 Q ${r2(baseX + 26)} 196 ${r2(baseX + 34)} 152" stroke="#24301f" stroke-width="2.4" fill="none" opacity=".2"/>`);
    for (let i = 0; i < 8; i++) {
      const t = i / 7;
      const cx = baseX + 4 + t * 74 + rand() * 14;
      const cy = 220 - t * 104 + rand() * 16;
      parts.push(`<ellipse cx="${r2(cx)}" cy="${r2(cy)}" rx="${r2(15 + rand() * 16)}" ry="${r2(6 + rand() * 8)}" fill="url(#leaf)" transform="rotate(${r2(-46 + rand() * 66)} ${r2(cx)} ${r2(cy)})"/>`);
    }
    parts.push(`<ellipse cx="${r2(baseX + 4)}" cy="240" rx="42" ry="24" fill="#1f2b1c" opacity=".34"/>`);
    parts.push('<rect x="0" y="206" width="320" height="34" fill="#6b5947" opacity=".13"/>');
    for (let i = 0; i < 14; i++) {
      parts.push(`<circle cx="${r2(rand() * 320)}" cy="${r2(20 + rand() * 180)}" r="${r2(.4 + rand() * 1)}" fill="#fff8e8" opacity="${r2(.08 + rand() * .12)}"/>`);
    }
  } else if (scene === 3) {
    // 海面薄雾：低对比、细浪
    const hor = 118 + rand() * 14;
    const sky = variant ? ['#8f9db0', '#d9c4ad', '#e7c6a8'] : ['#c3d3de', '#dcd9cf', '#f0e1cc'];
    const water = variant ? ['#5f7b88', '#233d4a'] : ['#7c96a0', '#2f4954'];
    defs.push(`<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${sky[0]}"/><stop offset=".7" stop-color="${sky[1]}"/><stop offset="1" stop-color="${sky[2]}"/></linearGradient>`);
    defs.push(`<linearGradient id="sea" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${water[0]}"/><stop offset="1" stop-color="${water[1]}"/></linearGradient>`);
    defs.push('<radialGradient id="gl"><stop offset="0" stop-color="#ffe9c8" stop-opacity=".8"/><stop offset="1" stop-color="#ffe9c8" stop-opacity="0"/></radialGradient>');
    parts.push('<rect width="320" height="240" fill="url(#sky)"/>');
    parts.push(`<ellipse cx="${r2(200 + rand() * 80)}" cy="${r2(hor - 12)}" rx="${r2(38 + rand() * 22)}" ry="${r2(7 + rand() * 5)}" fill="url(#gl)"/>`);
    parts.push(`<rect x="0" y="${r2(hor)}" width="320" height="${r2(240 - hor)}" fill="url(#sea)"/>`);
    parts.push(`<rect x="0" y="${r2(hor)}" width="320" height=".8" fill="#fff" opacity=".22"/>`);
    parts.push(`<path d="M0 ${r2(hor + 1)} Q 60 ${r2(hor - 4)} 120 ${r2(hor + 2)} T 320 ${r2(hor - 1)} V${r2(hor + 6)} H0Z" fill="#2c3e40" opacity=".22"/>`);
    for (let i = 0; i < 7; i++) {
      const y = hor + 8 + i * (100 / 7) + rand() * 4;
      parts.push(`<path d="M${r2(rand() * 40)} ${r2(y)} Q 90 ${r2(y - 2)} 170 ${r2(y + 1)} T 320 ${r2(y)}" stroke="#fff" stroke-width=".7" fill="none" opacity="${r2(.04 + rand() * .06)}"/>`);
    }
  } else {
    // 林间散景：深绿 + 一点金色光斑
    dark = true;
    defs.push('<linearGradient id="bg" x1="0" y1="0" x2=".3" y2="1"><stop offset="0" stop-color="#1a2a1c"/><stop offset="1" stop-color="#080f09"/></linearGradient>');
    const cols = variant ? ['#d9c58a', '#9fb07a', '#e6d7b4'] : ['#8fae6f', '#c9c08a', '#6f9068'];
    cols.forEach((c, i) => {
      defs.push(`<radialGradient id="k${i}"><stop offset="0" stop-color="${c}" stop-opacity=".9"/><stop offset=".5" stop-color="${c}" stop-opacity=".34"/><stop offset="1" stop-color="${c}" stop-opacity="0"/></radialGradient>`);
    });
    defs.push('<linearGradient id="canopy" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#040a05" stop-opacity=".82"/><stop offset="1" stop-color="#040a05" stop-opacity="0"/></linearGradient>');
    defs.push('<linearGradient id="trunk" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#040a06" stop-opacity="0"/><stop offset=".42" stop-color="#040a06" stop-opacity=".62"/><stop offset=".68" stop-color="#040a06" stop-opacity=".48"/><stop offset="1" stop-color="#040a06" stop-opacity="0"/></linearGradient>');
    parts.push('<rect width="320" height="240" fill="url(#bg)"/>');
    parts.push('<rect x="0" y="0" width="320" height="112" fill="url(#canopy)"/>');
    for (let i = 0; i < 20; i++) {
      const x = rand() * 320;
      const y = rand() * 240;
      const rad = 2 + rand() * rand() * 16;
      parts.push(`<circle cx="${r2(x)}" cy="${r2(y)}" r="${r2(rad)}" fill="url(#k${i % 3})" opacity="${r2(.18 + rand() * .42)}"/>`);
    }
    parts.push(`<rect x="${r2(-10 + rand() * 12)}" y="0" width="34" height="240" fill="url(#trunk)" opacity=".6"/>`);
    parts.push(`<rect x="${r2(286 + rand() * 10)}" y="0" width="38" height="240" fill="url(#trunk)" opacity=".45"/>`);
  }

  const grain = dark ? "#fff" : "#000";
  for (let i = 0; i < 110; i++) {
    parts.push(`<circle cx="${r2(rand() * 320)}" cy="${r2(rand() * 240)}" r="${r2(.25 + rand() * .7)}" fill="${grain}" opacity="${r2(.02 + rand() * .05)}"/>`);
  }
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 320 240">` +
    `<defs>${defs.join("")}</defs>${parts.join("")}<rect width="320" height="240" fill="url(#v)"/></svg>`;
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

// 有些日子拍了两场（上午出门 + 傍晚在家），用来验证「按事件成页」能断开
const SPLIT_DAYS = new Set([1, 10]);
// 还有一晚拍到过了午夜：按天会被拆成两行，按事件仍是一段
const NIGHT_DAY = 5;
// 还有一天下午换成了安卓手机拍：间隔不到 6 小时，但设备前缀变了，按事件应断开
const DEVICE_DAY = 3;

for (const plan of PLANS) {
  const base = new Date();
  const night = plan.ago === NIGHT_DAY;
  base.setHours(night ? 23 : 18, night ? 20 : 40, 0, 0);
  base.setDate(base.getDate() - plan.ago);
  const splitAt = SPLIT_DAYS.has(plan.ago) ? Math.floor(plan.count * 0.55) : -1;
  const deviceSplit = plan.ago === DEVICE_DAY ? Math.floor(plan.count * 0.6) : -1;
  let burst: number | null = null;
  let burstLeft = 0;
  for (let i = 0; i < plan.count; i++) {
    const android = deviceSplit > 0 && i >= deviceSplit;
    const t = night
      ? Math.floor(base.getTime() / 1000) + i * 480
      : android
        ? Math.floor(base.getTime() / 1000) - 4.33 * 3600 - (i - deviceSplit) * 80
        : i >= splitAt && splitAt > 0
          ? Math.floor(base.getTime() / 1000) - 9 * 3600 - (i - splitAt) * 70
          : Math.floor(base.getTime() / 1000) - i * 90 - (i % 3) * 40;
    // 换设备必换连拍组：否则一个连拍组跨过设备边界，按事件就永远不会断开
    if (android) {
      burst = null;
      burstLeft = 0;
    }
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
      path: android ? "/demo/PXL_" + (3000 + id) + ".jpg" : "/demo/IMG_" + (2000 + id) + ".HEIC",
      filename: android ? "PXL_" + (3000 + id) + ".jpg" : "IMG_" + (2000 + id) + ".HEIC",
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
  // 设计检查用：/?empty 直接看空状态
  if (typeof location !== "undefined" && new URLSearchParams(location.search).has("empty")) items.length = 0;

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
