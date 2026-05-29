import { useMemo, useState } from "react";
import { type CronFrequency, CRON_FREQ_OPTIONS, cronToSegments, segmentsToCron, describeCron, getNextCronRuns } from "./cron-utils";

export function CronScheduleSelector({ value, onChange }: { value: string; onChange: (cron: string) => void }) {
  const parsed = useMemo(() => cronToSegments(value), [value]);
  const [freq, setFreq] = useState<CronFrequency>(parsed.freq);
  const [interval, setIntervalVal] = useState(parsed.interval);
  const [hour, setHour] = useState(parsed.hour);
  const [minute, setMinute] = useState(parsed.minute);

  const isCustomMode = freq === "custom";

  const handleFreqChange = (f: CronFrequency) => {
    setFreq(f);
    if (f === "custom") return;
    onChange(segmentsToCron(f, interval, hour, minute));
  };

  const handleParamChange = (newInterval: number, newHour: number, newMinute: number) => {
    setIntervalVal(newInterval);
    setHour(newHour);
    setMinute(newMinute);
    onChange(segmentsToCron(freq, newInterval, newHour, newMinute));
  };

  return (
    <div className="space-y-2.5">
      <div>
        <label className="text-[11px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">频率</label>
        <select
          value={freq}
          onChange={(e) => handleFreqChange(e.target.value as CronFrequency)}
          className="agentdash-form-select mt-1"
        >
          {CRON_FREQ_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
      </div>
      {freq === "every_n_min" && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">每隔</span>
          <input type="number" value={interval} onChange={(e) => handleParamChange(Math.max(1, Number(e.target.value) || 1), hour, minute)} min={1} max={59} className="agentdash-form-input w-16" />
          <span className="text-xs text-muted-foreground">分钟</span>
        </div>
      )}
      {freq === "every_n_hour" && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">每隔</span>
          <input type="number" value={interval} onChange={(e) => handleParamChange(Math.max(1, Number(e.target.value) || 1), hour, minute)} min={1} max={23} className="agentdash-form-input w-16" />
          <span className="text-xs text-muted-foreground">小时</span>
        </div>
      )}
      {(freq === "daily" || freq === "weekday") && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">时间</span>
          <input type="number" value={hour} onChange={(e) => handleParamChange(interval, Math.min(23, Math.max(0, Number(e.target.value) || 0)), minute)} min={0} max={23} className="agentdash-form-input w-14" />
          <span className="text-xs text-muted-foreground">:</span>
          <input type="number" value={minute} onChange={(e) => handleParamChange(interval, hour, Math.min(59, Math.max(0, Number(e.target.value) || 0)))} min={0} max={59} className="agentdash-form-input w-14" />
        </div>
      )}
      {isCustomMode && (
        <div>
          <label className="text-[11px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">Cron 表达式</label>
          <input value={value} onChange={(e) => onChange(e.target.value)} placeholder="0 */2 * * 1-5" className="agentdash-form-input mt-1 font-mono" />
          <p className="mt-1.5 text-[10px] text-muted-foreground">
            格式: 分 时 日 月 周 — 如 <code className="font-mono">0 */2 * * 1-5</code> 表示工作日每 2 小时
          </p>
        </div>
      )}
      {!isCustomMode && (
        <div className="flex items-center gap-2">
          <code className="rounded-[6px] bg-secondary/50 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">{segmentsToCron(freq, interval, hour, minute)}</code>
          <span className="text-[10px] text-muted-foreground/70">{describeCron(freq, interval, hour, minute)}</span>
        </div>
      )}

      {/* Next runs preview */}
      <NextRunsPreview cron={isCustomMode ? value : segmentsToCron(freq, interval, hour, minute)} />
    </div>
  );
}

function NextRunsPreview({ cron }: { cron: string }) {
  const nextRuns = useMemo(() => getNextCronRuns(cron, 3), [cron]);

  if (!nextRuns) return null;

  const formatTime = (d: Date) => {
    const pad = (n: number) => String(n).padStart(2, "0");
    const mon = pad(d.getMonth() + 1);
    const day = pad(d.getDate());
    const h = pad(d.getHours());
    const m = pad(d.getMinutes());
    const weekdays = ["日", "一", "二", "三", "四", "五", "六"];
    return `${mon}-${day} (周${weekdays[d.getDay()]}) ${h}:${m}`;
  };

  return (
    <div className="rounded-[6px] border border-border/50 bg-secondary/10 px-2.5 py-2">
      <p className="text-[10px] font-medium text-muted-foreground mb-1">接下来触发</p>
      <ul className="space-y-0.5">
        {nextRuns.map((d, i) => (
          <li key={i} className="flex items-center gap-1.5 text-[11px] text-foreground/80">
            <span className="inline-block h-1 w-1 rounded-[4px] bg-info/60" />
            {formatTime(d)}
          </li>
        ))}
      </ul>
    </div>
  );
}
