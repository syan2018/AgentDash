import { useDebugPrefs } from "../../../hooks/use-debug-prefs";
import { SectionCard } from "./primitives";

export function DebugPrefsSection() {
  const { prefs, setHookVerbose } = useDebugPrefs();
  return (
    <SectionCard title="开发者选项">
      <p className="text-xs text-muted-foreground -mt-2">
        本地调试偏好，仅存储在当前浏览器，不影响其他用户。
      </p>
      <label className="flex items-center gap-3 cursor-pointer">
        <input
          type="checkbox"
          checked={prefs.hookVerbose}
          onChange={(e) => setHookVerbose(e.target.checked)}
          className="h-4 w-4 rounded border-border accent-primary"
        />
        <div>
          <span className="text-sm text-foreground">Hook Verbose 模式</span>
          <p className="text-xs text-muted-foreground">
            开启后，会话事件流中将显示所有 Hook 决策（包括 noop、allow、dispatched 等通常被过滤的静默事件），便于调试 Hook 规则链路。
          </p>
        </div>
      </label>
    </SectionCard>
  );
}
