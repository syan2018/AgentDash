import { useMemo, useState } from "react";
import { type CronFrequency, CRON_FREQ_OPTIONS, cronToSegments, segmentsToCron, describeCron } from "./cron-utils";

export function CronScheduleSelector({ value, onChange }: { value: string; onChange: (cron: string) => void }) {
  const parsed = useMemo(() => cronToSegments(value), [value]);
  const isCustom = value.trim() !== "" && !["every_n_min", "every_n_hour", "daily", "weekday"].includes(parsed.freq);
  const [freq, setFreq] = useState<CronFrequency>(parsed.freq);
  const [interval, setIntervalVal] = useState(parsed.interval);
  const [hour, setHour] = useState(parsed.hour);
  const [minute, setMinute] = useState(parsed.minute);
  const [showRaw, setShowRaw] = useState(isCustom);

  const handleFreqChange = (f: CronFrequency) => {
    setFreq(f);
    setShowRaw(false);
    onChange(segmentsToCron(f, interval, hour, minute));
  };

  const handleParamChange = (newInterval: number, newHour: number, newMinute: number) => {
    setIntervalVal(newInterval);
    setHour(newHour);
    setMinute(newMinute);
    onChange(segmentsToCron(freq, newInterval, newHour, newMinute));
  };

  const generatedCron = segmentsToCron(freq, interval, hour, minute);

  return (
    <div className="space-y-2.5">
      <div>
        <label className="text-[11px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">频率</label>
        <select
          value={showRaw ? undefined : freq}
          onChange={(e) => handleFreqChange(e.target.value as CronFrequency)}
          className="agentdash-form-select mt-1"
        >
          {CRON_FREQ_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
      </div>
      {freq === "every_n_min" && !showRaw && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">每隔</span>
          <input type="number" value={interval} onChange={(e) => handleParamChange(Math.max(1, Number(e.target.value) || 1), hour, minute)} min={1} max={59} className="agentdash-form-input w-16" />
          <span className="text-xs text-muted-foreground">分钟</span>
        </div>
      )}
      {freq === "every_n_hour" && !showRaw && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">每隔</span>
          <input type="number" value={interval} onChange={(e) => handleParamChange(Math.max(1, Number(e.target.value) || 1), hour, minute)} min={1} max={23} className="agentdash-form-input w-16" />
          <span className="text-xs text-muted-foreground">小时</span>
        </div>
      )}
      {(freq === "daily" || freq === "weekday") && !showRaw && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">时间</span>
          <input type="number" value={hour} onChange={(e) => handleParamChange(interval, Math.min(23, Math.max(0, Number(e.target.value) || 0)), minute)} min={0} max={23} className="agentdash-form-input w-14" />
          <span className="text-xs text-muted-foreground">:</span>
          <input type="number" value={minute} onChange={(e) => handleParamChange(interval, hour, Math.min(59, Math.max(0, Number(e.target.value) || 0)))} min={0} max={59} className="agentdash-form-input w-14" />
        </div>
      )}
      {showRaw && (
        <div>
          <label className="text-[11px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">Cron 表达式</label>
          <input value={value} onChange={(e) => onChange(e.target.value)} placeholder="* * * * *" className="agentdash-form-input mt-1 font-mono" />
        </div>
      )}
      {!showRaw && (
        <div className="flex items-center gap-2">
          <code className="rounded-[6px] bg-secondary/50 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">{generatedCron}</code>
          <span className="text-[10px] text-muted-foreground/70">{describeCron(freq, interval, hour, minute)}</span>
        </div>
      )}
      {isCustom && !showRaw && (
        <p className="text-[10px] text-warning">
          当前为自定义表达式：<code className="font-mono">{value}</code>
          <button type="button" onClick={() => setShowRaw(true)} className="ml-1 underline hover:no-underline">手动编辑</button>
        </p>
      )}
    </div>
  );
}
