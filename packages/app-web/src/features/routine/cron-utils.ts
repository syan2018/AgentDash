export type CronFrequency = "every_n_min" | "every_n_hour" | "daily" | "weekday" | "custom";

export const CRON_FREQ_OPTIONS: Array<{ value: CronFrequency; label: string }> = [
  { value: "every_n_min", label: "每隔 N 分钟" },
  { value: "every_n_hour", label: "每隔 N 小时" },
  { value: "daily", label: "每天指定时间" },
  { value: "weekday", label: "工作日指定时间" },
  { value: "custom", label: "自定义 Cron" },
];

export function cronToSegments(cron: string): { freq: CronFrequency; interval: number; hour: number; minute: number } {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return { freq: "custom", interval: 10, hour: 9, minute: 0 };
  const [mm, hh, , , dow] = parts;
  if (mm.startsWith("*/") && hh === "*") {
    const n = Number(mm.slice(2));
    if (Number.isFinite(n) && n > 0) return { freq: "every_n_min", interval: n, hour: 9, minute: 0 };
  }
  if (hh.startsWith("*/") && /^\d+$/.test(mm)) {
    const n = Number(hh.slice(2));
    if (Number.isFinite(n) && n > 0) return { freq: "every_n_hour", interval: n, hour: 9, minute: Number(mm) };
  }
  if (/^\d+$/.test(mm) && /^\d+$/.test(hh)) {
    const h = Number(hh);
    const m = Number(mm);
    if (dow === "1-5") return { freq: "weekday", interval: 10, hour: h, minute: m };
    if (dow === "*") return { freq: "daily", interval: 10, hour: h, minute: m };
  }
  return { freq: "custom", interval: 10, hour: 9, minute: 0 };
}

export function segmentsToCron(freq: CronFrequency, interval: number, hour: number, minute: number): string {
  switch (freq) {
    case "every_n_min": return `*/${Math.max(1, interval)} * * * *`;
    case "every_n_hour": return `${minute} */${Math.max(1, interval)} * * *`;
    case "daily": return `${minute} ${hour} * * *`;
    case "weekday": return `${minute} ${hour} * * 1-5`;
    case "custom": return "";
  }
}

export function describeCron(freq: CronFrequency, interval: number, hour: number, minute: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  switch (freq) {
    case "every_n_min": return `每 ${interval} 分钟执行一次`;
    case "every_n_hour": return `每 ${interval} 小时执行一次（在第 ${minute} 分钟）`;
    case "daily": return `每天 ${pad(hour)}:${pad(minute)} 执行`;
    case "weekday": return `工作日 ${pad(hour)}:${pad(minute)} 执行`;
    case "custom": return "自定义表达式";
  }
}

// ─── Next Run Preview ───

function parseCronField(field: string, min: number, max: number): number[] {
  const results = new Set<number>();
  for (const part of field.split(",")) {
    const stepMatch = part.match(/^(.+)\/(\d+)$/);
    const step = stepMatch ? Number(stepMatch[2]) : 1;
    const range = stepMatch ? stepMatch[1] : part;

    if (range === "*") {
      for (let i = min; i <= max; i += step) results.add(i);
    } else if (range.includes("-")) {
      const [a, b] = range.split("-").map(Number);
      for (let i = a; i <= b; i += step) results.add(i);
    } else {
      results.add(Number(range));
    }
  }
  return [...results].filter((n) => n >= min && n <= max).sort((a, b) => a - b);
}

export function getNextCronRuns(cron: string, count: number, from?: Date): Date[] | null {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return null;

  const [minField, hourField, domField, monField, dowField] = parts;

  let minutes: number[];
  let hours: number[];
  let doms: number[];
  let months: number[];
  let dows: number[];
  try {
    minutes = parseCronField(minField, 0, 59);
    hours = parseCronField(hourField, 0, 23);
    doms = parseCronField(domField, 1, 31);
    months = parseCronField(monField, 1, 12);
    dows = parseCronField(dowField, 0, 6);
  } catch {
    return null;
  }

  if (!minutes.length || !hours.length || !doms.length || !months.length || !dows.length) return null;

  const domIsWild = domField === "*";
  const dowIsWild = dowField === "*";

  const results: Date[] = [];
  const start = from ?? new Date();
  const cursor = new Date(start);
  cursor.setSeconds(0, 0);
  cursor.setMinutes(cursor.getMinutes() + 1);

  const maxIterations = 525960; // ~1 year of minutes
  for (let i = 0; i < maxIterations && results.length < count; i++) {
    const m = cursor.getMinutes();
    const h = cursor.getHours();
    const dom = cursor.getDate();
    const mon = cursor.getMonth() + 1;
    const dow = cursor.getDay();

    const minuteMatch = minutes.includes(m);
    const hourMatch = hours.includes(h);
    const monthMatch = months.includes(mon);

    // Standard cron: if both DOM and DOW are restricted, match is OR; if one is *, the other decides
    let dateMatch: boolean;
    if (domIsWild && dowIsWild) {
      dateMatch = true;
    } else if (domIsWild) {
      dateMatch = dows.includes(dow);
    } else if (dowIsWild) {
      dateMatch = doms.includes(dom);
    } else {
      dateMatch = doms.includes(dom) || dows.includes(dow);
    }

    if (minuteMatch && hourMatch && monthMatch && dateMatch) {
      results.push(new Date(cursor));
    }

    cursor.setMinutes(cursor.getMinutes() + 1);
  }

  return results.length > 0 ? results : null;
}
