import type { CapabilityDirective, CapabilityKey } from "../../../types";
import {
  CAPABILITY_OPTIONS,
  directiveKind,
  directivePath,
  parseCapabilityPath,
} from "../../../types";
import { replaceWellKnownCapabilitySelection } from "./form-state";

const WELL_KNOWN_CAPABILITY_KEYS = new Set<CapabilityKey>(
  CAPABILITY_OPTIONS.map((option) => option.value),
);

function isCapabilityKey(value: string): value is CapabilityKey {
  return WELL_KNOWN_CAPABILITY_KEYS.has(value as CapabilityKey);
}

function readWellKnownCapabilityState(directives: CapabilityDirective[]): {
  selected: CapabilityKey[];
  explicitAddCount: number;
} {
  const latest = new Map<CapabilityKey, "add" | "remove">();
  let explicitAddCount = 0;
  for (const directive of directives) {
    try {
      const path = parseCapabilityPath(directivePath(directive));
      if (path.tool !== null || !isCapabilityKey(path.capability)) continue;
      const kind = directiveKind(directive);
      latest.set(path.capability, kind);
      if (kind === "add") explicitAddCount += 1;
    } catch {
      // 非法 path 不是此 Tab 的编辑对象，保留给保存路径原样处理。
    }
  }

  if (explicitAddCount > 0) {
    return {
      selected: CAPABILITY_OPTIONS
        .map((option) => option.value)
        .filter((key) => latest.get(key) === "add"),
      explicitAddCount,
    };
  }

  return {
    selected: CAPABILITY_OPTIONS
      .map((option) => option.value)
      .filter((key) => latest.get(key) !== "remove"),
    explicitAddCount,
  };
}

export function ToolCapabilitiesField({
  directives,
  onChange,
}: {
  directives: CapabilityDirective[];
  onChange: (next: CapabilityDirective[]) => void;
}) {
  const { selected } = readWellKnownCapabilityState(directives);
  const isAll = selected.length >= CAPABILITY_OPTIONS.length;
  const has = (v: CapabilityKey) => selected.includes(v);

  const toggle = (v: CapabilityKey) => {
    if (isAll) {
      onChange(replaceWellKnownCapabilitySelection(
        directives,
        CAPABILITY_OPTIONS.map((o) => o.value).filter((c) => c !== v),
      ));
      return;
    }
    const next = selected.includes(v)
      ? selected.filter((c) => c !== v)
      : [...selected, v];
    onChange(replaceWellKnownCapabilitySelection(directives, next));
  };

  const basicOpts = CAPABILITY_OPTIONS.filter((o) => o.group === "basic");
  const extOpts = CAPABILITY_OPTIONS.filter((o) => o.group === "extended");

  return (
    <div className="space-y-3">
      {/* ── basic: horizontal pill toggles ── */}
      <div>
        <label className="agentdash-form-label">基础能力</label>
        <div className="flex flex-wrap gap-1.5">
          {basicOpts.map((opt) => {
            const on = has(opt.value);
            return (
              <button
                key={opt.value}
                type="button"
                onClick={() => toggle(opt.value)}
                className={`rounded-[8px] border px-3 py-1.5 text-xs font-medium transition-all duration-160 ${
                  on
                    ? "border-primary/30 bg-primary/8 text-primary"
                    : "border-border bg-secondary/30 text-muted-foreground hover:border-primary/20 hover:text-foreground"
                }`}
                title={opt.description}
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* ── extended: vertical rows with toggle switches ── */}
      <div>
        <label className="agentdash-form-label">扩展能力</label>
        <div className="grid grid-cols-1 gap-0.5 rounded-[8px] border border-border bg-secondary/20 p-2.5 md:grid-cols-2 md:gap-x-2">
          {extOpts.map((opt) => {
            const on = has(opt.value);
            return (
              <label
                key={opt.value}
                className={`flex cursor-pointer items-center gap-2.5 rounded-[8px] px-2.5 py-[7px] transition-all duration-160 ${
                  on
                    ? "bg-primary/6"
                    : "opacity-45 hover:opacity-70"
                }`}
              >
                <span className="relative inline-flex h-[18px] w-[32px] shrink-0">
                  <input
                    type="checkbox"
                    checked={on}
                    onChange={() => toggle(opt.value)}
                    className="peer sr-only"
                  />
                  {/* eslint-disable-next-line no-restricted-syntax -- 开关轨道为药丸形态 */}
                  <span className="absolute inset-0 rounded-full bg-border transition-colors duration-160 peer-checked:bg-primary" />
                  {/* eslint-disable-next-line no-restricted-syntax -- 开关旋钮为圆形 */}
                  <span className="absolute left-[3px] top-[3px] h-3 w-3 rounded-full bg-background shadow-sm transition-transform duration-160 peer-checked:translate-x-[14px]" />
                </span>
                <span className="text-xs font-medium text-foreground">{opt.label}</span>
                <span className="text-[10px] text-muted-foreground">{opt.description}</span>
              </label>
            );
          })}
        </div>
      </div>
    </div>
  );
}
