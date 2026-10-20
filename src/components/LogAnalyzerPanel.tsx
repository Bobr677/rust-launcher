import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { useT, type Language } from "../i18n";
import {
  analyzeGameLogs,
  type LogAnalyzerFinding,
  type LogAnalyzerInputSource,
  type LogAnalyzerSeverity,
} from "../lib/logAnalyzer";
import { INSTALL_CONSOLE_PROFILE_ID } from "../lib/gameConsoleWindow";

type DiskLogSourceDto = {
  kind: string;
  name: string;
  path: string;
  content: string;
  truncated: boolean;
  modifiedAt: number;
};

type InstanceAnalysisLogsDto = {
  sources: DiskLogSourceDto[];
};

type LogAnalyzerPanelProps = {
  lines: string[];
  language: Language;
  /** Instance/profile id whose logs/ and crash-reports/ to read. */
  profileId?: string | null;
  /** When game crashes, force a disk re-read. */
  gameStatus?: string;
  className?: string;
  /** Compact layout for narrow manage/console columns. */
  compact?: boolean;
};

function severityClass(severity: LogAnalyzerSeverity): string {
  switch (severity) {
    case "error":
      return "border-red-400/25 bg-red-500/[0.08] text-red-100/90";
    case "warning":
      return "border-amber-400/25 bg-amber-500/[0.08] text-amber-100/90";
    default:
      return "border-sky-400/25 bg-sky-500/[0.08] text-sky-100/90";
  }
}

function severityDot(severity: LogAnalyzerSeverity): string {
  switch (severity) {
    case "error":
      return "bg-red-400";
    case "warning":
      return "bg-amber-400";
    default:
      return "bg-sky-400";
  }
}

function FindingCard({
  finding,
  tt,
  compact,
}: {
  finding: LogAnalyzerFinding;
  tt: (key: string, vars?: Record<string, string | number>) => string;
  compact?: boolean;
}) {
  return (
    <div
      className={`border px-2.5 py-2 ${severityClass(finding.severity)} ${
        compact ? "rounded-lg" : "rounded-xl"
      }`}
    >
      <div className="flex items-start gap-2">
        <span
          className={`mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full ${severityDot(finding.severity)}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-2">
            <p className="text-[11px] font-semibold leading-snug">
              {tt(finding.titleKey)}
            </p>
            {finding.matchCount > 1 ? (
              <span className="shrink-0 rounded bg-black/25 px-1.5 py-0.5 text-[10px] text-white/55">
                ×{finding.matchCount}
              </span>
            ) : null}
          </div>
          <p className="mt-0.5 text-[11px] leading-snug text-white/70">
            {tt(finding.fixKey)}
          </p>
          {finding.detail ? (
            <p
              className="mt-1 line-clamp-2 font-mono text-[10px] leading-snug text-white/55"
              title={finding.detail}
            >
              {finding.detail}
            </p>
          ) : finding.sample ? (
            <p
              className="mt-1 truncate font-mono text-[10px] text-white/35"
              title={finding.sample}
            >
              {finding.sample}
            </p>
          ) : null}
          {finding.sources && finding.sources.length > 0 ? (
            <p className="mt-1 text-[9px] uppercase tracking-wide text-white/30">
              {finding.sources.join(" · ")}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function toInputSources(disk: DiskLogSourceDto[]): LogAnalyzerInputSource[] {
  return disk.map((s) => ({
    id: s.kind === "latest" ? "latest" : `crash:${s.name}`,
    label: s.kind === "latest" ? "latest.log" : s.name,
    text: s.content,
  }));
}

export function LogAnalyzerPanel({
  lines,
  language,
  profileId = null,
  gameStatus,
  className = "",
  compact = false,
}: LogAnalyzerPanelProps) {
  const tt = useT(language);
  const [open, setOpen] = useState(false);
  const [diskSources, setDiskSources] = useState<DiskLogSourceDto[]>([]);
  const [diskLoading, setDiskLoading] = useState(false);
  const [diskError, setDiskError] = useState(false);
  const [reloadTick, setReloadTick] = useState(0);

  const canReadDisk =
    !!profileId &&
    profileId !== INSTALL_CONSOLE_PROFILE_ID &&
    !profileId.startsWith("__");

  useEffect(() => {
    if (!canReadDisk || !profileId) {
      setDiskSources([]);
      setDiskError(false);
      return;
    }

    let cancelled = false;
    setDiskLoading(true);
    setDiskError(false);

    void (async () => {
      try {
        const result = await invoke<InstanceAnalysisLogsDto>(
          "read_instance_analysis_logs",
          { profileId },
        );
        if (cancelled) return;
        setDiskSources(result.sources ?? []);
      } catch {
        if (!cancelled) {
          setDiskSources([]);
          setDiskError(true);
        }
      } finally {
        if (!cancelled) setDiskLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [canReadDisk, profileId, gameStatus, reloadTick]);

  // Re-read disk logs shortly after a crash — Minecraft may still be flushing files.
  useEffect(() => {
    if (gameStatus !== "crashed" || !canReadDisk) return;
    const t1 = window.setTimeout(() => setReloadTick((n) => n + 1), 1500);
    const t2 = window.setTimeout(() => setReloadTick((n) => n + 1), 4000);
    return () => {
      window.clearTimeout(t1);
      window.clearTimeout(t2);
    };
  }, [gameStatus, canReadDisk]);

  const inputSources = useMemo(() => toInputSources(diskSources), [diskSources]);

  const findings = useMemo(
    () => analyzeGameLogs(lines, inputSources),
    [lines, inputSources],
  );

  const errorCount = useMemo(
    () => findings.filter((f) => f.severity === "error").length,
    [findings],
  );

  const sourceSummary = useMemo(() => {
    return [
      lines.length > 0 ? tt("play.console.analyzer.sourceConsole") : null,
      diskSources.some((s) => s.kind === "latest")
        ? tt("play.console.analyzer.sourceLatest")
        : null,
      diskSources.some((s) => s.kind === "crash_report")
        ? tt("play.console.analyzer.sourceCrash", {
            count: diskSources.filter((s) => s.kind === "crash_report").length,
          })
        : null,
    ].filter(Boolean) as string[];
  }, [lines.length, diskSources, tt]);

  // Auto-expand when we discover real issues (especially after a crash).
  useEffect(() => {
    if (findings.length > 0 && (gameStatus === "crashed" || errorCount > 0)) {
      setOpen(true);
    }
  }, [findings.length, gameStatus, errorCount]);

  const hasAnyInput = lines.length > 0 || diskSources.length > 0;
  if (!hasAnyInput && !diskLoading) return null;

  return (
    <div className={`shrink-0 ${className}`}>
      <div
        className={`border border-white/10 bg-white/[0.03] ${
          compact ? "rounded-lg" : "rounded-xl"
        }`}
      >
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className="interactive-press flex w-full items-center justify-between gap-2 px-2.5 py-1.5 text-left hover:bg-white/[0.04]"
        >
          <span className="flex min-w-0 items-center gap-2">
            <span className="text-[11px] font-semibold text-white/80">
              {tt("play.console.analyzer.title")}
            </span>
            {diskLoading ? (
              <span className="text-[10px] text-white/40">
                {tt("play.console.analyzer.readingDisk")}
              </span>
            ) : findings.length > 0 ? (
              <span
                className={`rounded px-1.5 py-0.5 text-[10px] font-semibold ${
                  errorCount > 0
                    ? "bg-red-500/20 text-red-200/90"
                    : "bg-amber-500/20 text-amber-100/90"
                }`}
              >
                {findings.length}
              </span>
            ) : (
              <span className="text-[10px] text-white/35">
                {tt("play.console.analyzer.okBadge")}
              </span>
            )}
          </span>
          <span className="flex shrink-0 items-center gap-2">
            {canReadDisk ? (
              <span
                role="button"
                tabIndex={0}
                onClick={(e) => {
                  e.stopPropagation();
                  setReloadTick((n) => n + 1);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    e.stopPropagation();
                    setReloadTick((n) => n + 1);
                  }
                }}
                className="interactive-press rounded px-1.5 py-0.5 text-[10px] text-white/45 hover:bg-white/10 hover:text-white/70"
                title={tt("play.console.analyzer.refresh")}
              >
                {tt("play.console.analyzer.refresh")}
              </span>
            ) : null}
            <span className="text-[10px] text-white/40">
              {open
                ? tt("play.console.analyzer.hide")
                : tt("play.console.analyzer.show")}
            </span>
          </span>
        </button>

        {open ? (
          <div className="border-t border-white/8 px-2.5 pb-2 pt-2">
            {sourceSummary.length > 0 ? (
              <p className="mb-2 text-[10px] text-white/35">
                {tt("play.console.analyzer.sourcesLabel")}:{" "}
                {sourceSummary.join(" · ")}
                {diskError ? (
                  <span className="text-amber-200/70">
                    {" "}
                    · {tt("play.console.analyzer.diskReadFailed")}
                  </span>
                ) : null}
              </p>
            ) : null}

            {findings.length > 0 ? (
              <div
                className={`flex flex-col gap-1.5 overflow-y-auto custom-scrollbar ${
                  compact ? "max-h-36" : "max-h-40"
                }`}
              >
                {findings.map((finding) => (
                  <FindingCard
                    key={finding.id}
                    finding={finding}
                    tt={tt}
                    compact={compact}
                  />
                ))}
              </div>
            ) : (
              <p className="text-[11px] leading-snug text-white/45">
                {tt("play.console.analyzer.noIssues")}
              </p>
            )}
          </div>
        ) : null}
      </div>
    </div>
  );
}
