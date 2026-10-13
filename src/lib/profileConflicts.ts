export type ConflictActionKind =
  | "disable_all_but"
  | "keep_latest"
  | "disable_all"
  | "disable_wrong_loader";

export type ProfileConflict = {
  id: string;
  severity: "warning" | "error";
  title: string;
  reason: string;
  affected: string[];
  solutions: string[];
  category?: "duplicate" | "loader" | "incompatible" | "dependency" | "breaks";
  actions?: { id: string; label: string; kind: ConflictActionKind; keep?: string }[];
};

export type JarModRelation = {
  modId: string;
  optional: boolean;
};

export type JarModInfo = {
  filename: string;
  enabled: boolean;
  modId?: string | null;
  title?: string | null;
  version?: string | null;
  loader?: string | null;
  provides?: string[];
  depends?: JarModRelation[];
  breaks?: JarModRelation[];
};

type Tt = (key: string, vars?: Record<string, string | number>) => string;

const PLATFORM_IDS = new Set([
  "minecraft",
  "java",
  "fabricloader",
  "fabric-loader",
  "quilt_loader",
  "quilt-loader",
  "forge",
  "neoforge",
  "mcp",
  "fabric",
  "quilt",
]);

function stripExt(s: string) {
  return s.replace(/\.(jar|zip|disabled)$/i, "").replace(/\.disabled$/i, "");
}

function extractVersionParts(filename: string): number[] | null {
  const base = stripExt(filename);
  const candidates = [...base.matchAll(/(?:^|[-_])(\d+(?:\.\d+){0,4})(?=[-_.]|$)/g)].map(
    (m) => m[1],
  );
  let best: number[] | null = null;
  for (const c of candidates) {
    const parts = c
      .split(".")
      .map((p) => Number.parseInt(p, 10))
      .filter((n) => Number.isFinite(n));
    if (parts.length === 0) continue;
    if (!best || parts.length >= best.length) best = parts;
  }
  if (best) return best;
  const m = base.match(/(\d+(?:\.\d+){0,4})/);
  if (!m) return null;
  const parts = m[1]
    .split(".")
    .map((p) => Number.parseInt(p, 10))
    .filter((n) => Number.isFinite(n));
  return parts.length > 0 ? parts : null;
}

function cmpVersionParts(a: number[], b: number[]): number {
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const av = a[i] ?? 0;
    const bv = b[i] ?? 0;
    if (av !== bv) return av < bv ? -1 : 1;
  }
  return 0;
}

export function pickLatestFilename(files: string[]): string | null {
  let best: { file: string; ver: number[] | null } | null = null;
  for (const f of files) {
    const ver = extractVersionParts(f);
    if (!best) {
      best = { file: f, ver };
      continue;
    }
    if (!best.ver && ver) {
      best = { file: f, ver };
      continue;
    }
    if (best.ver && ver && cmpVersionParts(best.ver, ver) < 0) {
      best = { file: f, ver };
    }
  }
  return best?.file ?? null;
}

export function guessLoaderVariant(filename: string): string | null {
  const f = stripExt(filename).toLowerCase();
  if (/(^|[-_.])(neoforge)([-_.]|$)/.test(f)) return "neoforge";
  if (/(^|[-_.])(forge)([-_.]|$)/.test(f)) return "forge";
  if (/(^|[-_.])(fabric)([-_.]|$)/.test(f)) return "fabric";
  if (/(^|[-_.])(quilt)([-_.]|$)/.test(f)) return "quilt";
  return null;
}

export function guessModKey(filename: string): string {
  let base = stripExt(filename).trim().toLowerCase();
  base = base
    .replace(/(^|[-_.])(neoforge|forge|fabric|quilt|bukkit|spigot|paper)(?=[-_.]|$)/g, "$1")
    .replace(/(^|[-_.])mc\d+(?:\.\d+)*(?=[-_.]|$)/g, "$1")
    .replace(/(^|[-_.])\d+\.\d+(?:\.\d+)*(?=[-_.]|$)/g, "$1")
    .replace(/(^|[-_.])(release|beta|alpha|rc\d*|snapshot|dev|build)(?=[-_.]|$)/gi, "$1")
    .replace(/[-_.]+/g, "-")
    .replace(/^-|-$/g, "");

  const idx = base.search(/\d/);
  const raw = (idx > 2 ? base.slice(0, idx) : base).replace(/[-_.]+$/g, "");
  return raw.trim().toLowerCase();
}

function normalizeId(id: string) {
  return id.trim().toLowerCase();
}

function collectProvidedIds(info: JarModInfo): string[] {
  const ids: string[] = [];
  if (info.modId) ids.push(normalizeId(info.modId));
  for (const p of info.provides ?? []) {
    if (p) ids.push(normalizeId(p));
  }
  return [...new Set(ids)];
}

export function detectProfileConflicts(opts: {
  entries: { name: string; enabled: boolean }[];
  profile: { loader?: string | null };
  jarInfos?: JarModInfo[];
  tt: Tt;
}): ProfileConflict[] {
  const { entries, profile, jarInfos = [], tt } = opts;
  const enabled = (entries ?? []).filter((e) => e && e.enabled && e.name);
  const conflicts: ProfileConflict[] = [];
  const claimedFilenames = new Set<string>();

  const infoByFile = new Map<string, JarModInfo>();
  for (const info of jarInfos) {
    if (info?.filename) infoByFile.set(info.filename, info);
  }

  const byModId = new Map<string, string[]>();
  for (const e of enabled) {
    const info = infoByFile.get(e.name);
    if (!info?.modId) continue;
    const id = normalizeId(info.modId);
    if (!id || PLATFORM_IDS.has(id)) continue;
    const arr = byModId.get(id) ?? [];
    arr.push(e.name);
    byModId.set(id, arr);
  }

  for (const [modId, files] of byModId.entries()) {
    if (files.length < 2) continue;
    const keepLatest = pickLatestFilename(files);
    const title =
      files.map((f) => infoByFile.get(f)?.title).find((t) => t && t.trim()) || modId;
    for (const f of files) claimedFilenames.add(f);
    conflicts.push({
      id: `modid:${modId}`,
      severity: "error",
      category: "duplicate",
      title: tt("modpacks.conflicts.duplicateId.title"),
      reason: tt("modpacks.conflicts.duplicateId.reason", { name: title, id: modId }),
      affected: files.slice(0, 12),
      solutions: [
        tt("modpacks.conflicts.duplicate.solutionDisableExtra"),
        tt("modpacks.conflicts.duplicate.solutionKeepLatest"),
      ],
      actions: [
        {
          id: "disable_extra",
          label: tt("modpacks.conflicts.actions.disableExtra"),
          kind: "disable_all_but",
        },
        ...(keepLatest
          ? [
              {
                id: "keep_latest",
                label: tt("modpacks.conflicts.actions.keepLatest"),
                kind: "keep_latest" as const,
                keep: keepLatest,
              },
            ]
          : []),
      ],
    });
  }

  const byKey = new Map<string, string[]>();
  const byKeyVariants = new Map<string, Set<string>>();
  for (const e of enabled) {
    if (claimedFilenames.has(e.name)) continue;
    const key = guessModKey(e.name);
    if (!key || key.length < 3) continue;
    const arr = byKey.get(key) ?? [];
    arr.push(e.name);
    byKey.set(key, arr);
    const metaLoader = infoByFile.get(e.name)?.loader?.toLowerCase() ?? null;
    const v = metaLoader || guessLoaderVariant(e.name);
    if (v) {
      const vs = byKeyVariants.get(key) ?? new Set<string>();
      vs.add(v);
      byKeyVariants.set(key, vs);
    }
  }

  for (const [key, files] of byKey.entries()) {
    if (files.length < 2) continue;
    const modIds = new Set(
      files
        .map((f) => infoByFile.get(f)?.modId)
        .filter((id): id is string => !!id)
        .map(normalizeId),
    );
    if (modIds.size >= 2) continue;

    const keepLatest = pickLatestFilename(files);
    conflicts.push({
      id: `dup:${key}`,
      severity: "error",
      category: "duplicate",
      title: tt("modpacks.conflicts.duplicate.title"),
      reason: tt("modpacks.conflicts.duplicate.reason", { name: key }),
      affected: files.slice(0, 12),
      solutions: [
        tt("modpacks.conflicts.duplicate.solutionDisableExtra"),
        tt("modpacks.conflicts.duplicate.solutionKeepLatest"),
      ],
      actions: [
        {
          id: "disable_extra",
          label: tt("modpacks.conflicts.actions.disableExtra"),
          kind: "disable_all_but",
        },
        ...(keepLatest
          ? [
              {
                id: "keep_latest",
                label: tt("modpacks.conflicts.actions.keepLatest"),
                kind: "keep_latest" as const,
                keep: keepLatest,
              },
            ]
          : []),
      ],
    });
  }

  const loader = (profile.loader ?? "").toLowerCase();
  if (loader && loader !== "vanilla") {
    const mismatchByVariant = new Map<string, string[]>();
    for (const e of enabled) {
      const info = infoByFile.get(e.name);
      const v = (info?.loader || guessLoaderVariant(e.name) || "").toLowerCase();
      if (!v) continue;
      const compatible =
        v === loader ||
        (loader === "fabric" && v === "quilt") ||
        (loader === "quilt" && v === "fabric");
      if (compatible) continue;
      const arr = mismatchByVariant.get(v) ?? [];
      arr.push(e.name);
      mismatchByVariant.set(v, arr);
    }

    for (const [variant, files] of mismatchByVariant.entries()) {
      conflicts.push({
        id: `loader:${variant}`,
        severity: "warning",
        category: "loader",
        title: tt("modpacks.conflicts.loaderMismatch.title"),
        reason: tt("modpacks.conflicts.loaderMismatch.reason", {
          loader,
          variants: variant,
        }),
        affected: files.slice(0, 12),
        solutions: [
          tt("modpacks.conflicts.loaderMismatch.solutionPickCorrect", { loader }),
          tt("modpacks.conflicts.loaderMismatch.solutionSwitchLoader"),
        ],
        actions: [
          {
            id: "disable_all",
            label: tt("modpacks.conflicts.actions.disableAll"),
            kind: "disable_all",
          },
        ],
      });
    }

    for (const [key, variants] of byKeyVariants.entries()) {
      if (variants.size <= 1) continue;
      const affected = (byKey.get(key) ?? []).slice(0, 12);
      conflicts.push({
        id: `mixed:${key}`,
        severity: "error",
        category: "loader",
        title: tt("modpacks.conflicts.mixedVariants.title"),
        reason: tt("modpacks.conflicts.mixedVariants.reason", {
          variants: [...variants].join(", "),
        }),
        affected,
        solutions: [
          tt("modpacks.conflicts.mixedVariants.solutionKeepOne"),
          tt("modpacks.conflicts.mixedVariants.solutionUseUniversal"),
        ],
        actions: [
          {
            id: "disable_wrong",
            label: tt("modpacks.conflicts.actions.disableWrongLoader"),
            kind: "disable_wrong_loader",
          },
        ],
      });
    }
  }

  const provided = new Set<string>();
  const enabledInfos: JarModInfo[] = [];
  for (const e of enabled) {
    const info = infoByFile.get(e.name);
    if (!info) continue;
    enabledInfos.push(info);
    for (const id of collectProvidedIds(info)) provided.add(id);
  }

  const missingByDep = new Map<string, string[]>();
  for (const info of enabledInfos) {
    for (const dep of info.depends ?? []) {
      if (!dep?.modId || dep.optional) continue;
      const id = normalizeId(dep.modId);
      if (!id || PLATFORM_IDS.has(id) || provided.has(id)) continue;
      const arr = missingByDep.get(id) ?? [];
      arr.push(info.filename);
      missingByDep.set(id, arr);
    }
  }

  for (const [depId, files] of missingByDep.entries()) {
    const uniqueFiles = [...new Set(files)];
    conflicts.push({
      id: `missing:${depId}`,
      severity: "error",
      category: "dependency",
      title: tt("modpacks.conflicts.missingDep.title"),
      reason: tt("modpacks.conflicts.missingDep.reason", { id: depId }),
      affected: uniqueFiles.slice(0, 12),
      solutions: [
        tt("modpacks.conflicts.missingDep.solutionInstall"),
        tt("modpacks.conflicts.missingDep.solutionDisable"),
      ],
      actions: [
        {
          id: "disable_all",
          label: tt("modpacks.conflicts.actions.disableDependents"),
          kind: "disable_all",
        },
      ],
    });
  }

  const breakPairs = new Map<string, { a: string; b: string; soft: boolean }>();
  for (const info of enabledInfos) {
    for (const br of info.breaks ?? []) {
      if (!br?.modId) continue;
      const id = normalizeId(br.modId);
      if (!id || PLATFORM_IDS.has(id) || !provided.has(id)) continue;
      const other = enabledInfos.find((o) => collectProvidedIds(o).includes(id));
      if (!other || other.filename === info.filename) continue;
      const pairKey = [info.filename, other.filename].sort().join("||");
      if (breakPairs.has(pairKey)) continue;
      breakPairs.set(pairKey, {
        a: info.filename,
        b: other.filename,
        soft: !!br.optional,
      });
    }
  }

  for (const [pairKey, pair] of breakPairs.entries()) {
    const titleA = infoByFile.get(pair.a)?.title || pair.a;
    const titleB = infoByFile.get(pair.b)?.title || pair.b;
    conflicts.push({
      id: `breaks:${pairKey}`,
      severity: pair.soft ? "warning" : "error",
      category: "breaks",
      title: tt("modpacks.conflicts.breaks.title"),
      reason: tt("modpacks.conflicts.breaks.reason", {
        a: String(titleA),
        b: String(titleB),
      }),
      affected: [pair.a, pair.b],
      solutions: [
        tt("modpacks.conflicts.breaks.solutionDisableOne"),
        tt("modpacks.conflicts.breaks.solutionFindAlt"),
      ],
      actions: [
        {
          id: "disable_a",
          label: tt("modpacks.conflicts.actions.disableFirst"),
          kind: "disable_all_but",
          keep: pair.b,
        },
        {
          id: "disable_b",
          label: tt("modpacks.conflicts.actions.disableSecond"),
          kind: "disable_all_but",
          keep: pair.a,
        },
      ],
    });
  }

  const catWeight: Record<string, number> = {
    duplicate: 0,
    breaks: 1,
    dependency: 2,
    loader: 3,
    incompatible: 4,
  };
  conflicts.sort((a, b) => {
    if (a.severity !== b.severity) return a.severity === "error" ? -1 : 1;
    return (catWeight[a.category ?? ""] ?? 9) - (catWeight[b.category ?? ""] ?? 9);
  });

  return conflicts;
}

export function conflictAffectedSet(conflicts: ProfileConflict[]): Set<string> {
  const set = new Set<string>();
  for (const c of conflicts) {
    for (const f of c.affected) set.add(f);
  }
  return set;
}
