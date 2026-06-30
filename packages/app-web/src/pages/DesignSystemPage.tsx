/**
 * DesignSystemPage — 设计语言可视化预览
 *
 * 路由：/dev/design-system
 * 任务：05-19-frontend-design-language
 * 用途：集中展示 token / radius / primitive / surface / 嵌套对比 / form 综合，
 *      作为本任务及后续设计调整的视觉验收基准。
 *
 * 本页不接 store / API，所有数据为 mock。
 */

import { useState, type ReactNode } from "react";
import {
  Badge,
  Button,
  Card,
  CardHeader,
  CardMenu,
  CreateButton,
  CheckboxField,
  ConfirmDialog,
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
  EmptyState,
  Field,
  InspectorRow,
  Notice,
  OriginBadge,
  type OriginBadgeTone,
  SectionTitle,
  Select,
  StatusDot,
  StatusScreen,
  TextInput,
  Textarea,
  PromptDialog,
  cn,
} from "@agentdash/ui";

// ────────────────────────────────────────────────────────
// Tokens
// ────────────────────────────────────────────────────────

const COLOR_TOKENS = [
  { name: "background", desc: "页面 / depth-0" },
  { name: "foreground", desc: "正文文字" },
  { name: "card", desc: "depth-1 容器底" },
  { name: "popover", desc: "弹层底色" },
  { name: "primary", desc: "主品牌 / info" },
  { name: "secondary", desc: "中性表面" },
  { name: "muted", desc: "muted bg" },
  { name: "muted-foreground", desc: "次要文字" },
  { name: "accent", desc: "强调 / 已发布" },
  { name: "destructive", desc: "danger" },
  { name: "warning", desc: "warning" },
  { name: "success", desc: "success" },
  { name: "info", desc: "info" },
  { name: "border", desc: "通用边框" },
];

// ────────────────────────────────────────────────────────
// 预览适配器：把页面用到的语义参数映射到 @agentdash/ui primitive
// ────────────────────────────────────────────────────────

type OriginKind =
  | "builtin_seed"
  | "user"
  | "github"
  | "clawhub"
  | "skills_sh"
  | "marketplace";

const ORIGIN_PREVIEW: Record<OriginKind, { label: string; tone: OriginBadgeTone }> = {
  builtin_seed: { label: "builtin", tone: "neutral" },
  user: { label: "user", tone: "accent" },
  github: { label: "github", tone: "info" },
  clawhub: { label: "clawhub", tone: "success" },
  skills_sh: { label: "skills.sh", tone: "warning" },
  marketplace: { label: "marketplace", tone: "success" },
};

function OriginBadgePreview({
  origin,
  subText,
}: {
  origin: OriginKind;
  subText?: string;
}) {
  const { label, tone } = ORIGIN_PREVIEW[origin];
  const display = subText ? `${label} · ${subText}` : label;
  return <OriginBadge label={display} tone={tone} />;
}

type DotTone = "success" | "warning" | "danger" | "info" | "muted";

function StatusDotPreview({
  tone,
  size = "sm",
  pulse = false,
  title,
}: {
  tone: DotTone;
  size?: "sm" | "md";
  pulse?: boolean;
  title?: string;
}) {
  return <StatusDot tone={tone} size={size} pulse={pulse} title={title} />;
}

function InspectorRowPreview({
  label,
  value,
  mono = false,
  tone = "default",
}: {
  label: string;
  value: ReactNode;
  mono?: boolean;
  tone?: "default" | "muted" | "success" | "warning" | "danger";
}) {
  const toneClass = {
    default: "",
    muted: "text-muted-foreground",
    success: "text-success",
    warning: "text-warning",
    danger: "text-destructive",
  }[tone];
  const wrapped =
    tone === "default" ? value : <span className={toneClass}>{value}</span>;
  return <InspectorRow label={label} value={wrapped} mono={mono} />;
}

function SectionTitlePreview({
  title,
  subtitle,
  badge,
  actions,
  sticky = false,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  badge?: ReactNode;
  actions?: ReactNode;
  sticky?: boolean;
}) {
  return (
    <SectionTitle
      title={title}
      subtitle={subtitle}
      badge={badge}
      actions={actions}
      sticky={sticky}
    />
  );
}

// ────────────────────────────────────────────────────────
// Page
// ────────────────────────────────────────────────────────

export function DesignSystemPage() {
  const [dark, setDark] = useState(false);

  return (
    <div className={cn("h-full overflow-y-auto", dark && "dark")}>
      <div className="min-h-full bg-background text-foreground">
        <PageHeader dark={dark} setDark={setDark} />

        <main className="mx-auto max-w-6xl space-y-16 px-6 pb-24 pt-8">
          <SectionTokens />
          <SectionRadius />
          <SectionPrimitives />
          <SectionSurface />
          <SectionElevationShell />
          <SectionNestingCompare />
          <SectionFormComposite />
        </main>
      </div>
    </div>
  );
}

export default DesignSystemPage;

// ────────────────────────────────────────────────────────
// Header
// ────────────────────────────────────────────────────────

function PageHeader({
  dark,
  setDark,
}: {
  dark: boolean;
  setDark: (v: boolean) => void;
}) {
  const anchors = [
    { id: "tokens", label: "1 · Tokens" },
    { id: "radius", label: "2 · Radius" },
    { id: "primitives", label: "3 · Primitives" },
    { id: "surface", label: "4 · Surface depth" },
    { id: "elevation", label: "5 · Elevation & Shell" },
    { id: "nesting", label: "6 · 嵌套对比" },
    { id: "form", label: "7 · Form 综合" },
  ];
  return (
    <header className="sticky top-0 z-20 border-b border-border/60 bg-background/85 backdrop-blur">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 px-6 py-3">
        <div>
          <p className="text-[10px] font-semibold uppercase tracking-[0.2em] text-muted-foreground">
            Design Language Preview
          </p>
          <h1 className="text-base font-semibold tracking-tight">
            /dev/design-system · 任务 05-19-frontend-design-language
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <nav className="hidden flex-wrap gap-1 lg:flex">
            {anchors.map((a) => (
              <a
                key={a.id}
                href={`#${a.id}`}
                className="rounded-[6px] px-2 py-1 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                {a.label}
              </a>
            ))}
          </nav>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setDark(!dark)}
            title="切换 dark / light 预览"
          >
            {dark ? "☀ light" : "☾ dark"}
          </Button>
        </div>
      </div>
    </header>
  );
}

// ────────────────────────────────────────────────────────
// Section 1 · Tokens
// ────────────────────────────────────────────────────────

function SectionTokens() {
  return (
    <SectionShell
      id="tokens"
      title="1 · Tokens"
      subtitle="HSL CSS 变量定义在 packages/ui/src/styles.css，所有颜色必须经此层访问，禁止 Tailwind 字面色（amber-500/30 等）。"
    >
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {COLOR_TOKENS.map((token) => (
          <ColorSwatch key={token.name} token={token.name} desc={token.desc} />
        ))}
      </div>
    </SectionShell>
  );
}

function ColorSwatch({ token, desc }: { token: string; desc: string }) {
  return (
    <div className="space-y-1.5">
      <div
        className="h-16 w-full rounded-[8px] border border-border/60"
        style={{ background: `hsl(var(--${token}))` }}
      />
      <div className="flex items-baseline justify-between gap-2">
        <code className="font-mono text-[11px] text-foreground">{token}</code>
        <span className="text-[10px] text-muted-foreground">{desc}</span>
      </div>
    </div>
  );
}

// ────────────────────────────────────────────────────────
// Section 2 · Radius
// ────────────────────────────────────────────────────────

const RADIUS_TOKENS = [
  { name: "xs", value: 4, use: "icon button" },
  { name: "sm", value: 6, use: "badge / pill / chip" },
  { name: "md", value: 8, use: "input / button / card / dialog / inspector row" },
  { name: "lg", value: 12, use: "outer dialog 外壳" },
];

function SectionRadius() {
  return (
    <SectionShell
      id="radius"
      title="2 · Radius"
      subtitle="4 / 6 / 8 / 12 是 sentinel 值，其他字面圆角（5 / 7 / 9 / 10 等）将触发 ESLint warn。"
    >
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        {RADIUS_TOKENS.map((r) => (
          <div key={r.name} className="space-y-2">
            <div
              className="flex h-20 items-center justify-center border border-border bg-secondary/40 text-xs text-muted-foreground"
              style={{ borderRadius: `${r.value}px` }}
            >
              {r.value}px
            </div>
            <div>
              <p className="font-mono text-[11px] text-foreground">{r.name}</p>
              <p className="text-[10px] text-muted-foreground">{r.use}</p>
            </div>
          </div>
        ))}
      </div>
    </SectionShell>
  );
}

// ────────────────────────────────────────────────────────
// Section 3 · Primitives
// ────────────────────────────────────────────────────────

function SectionPrimitives() {
  return (
    <SectionShell
      id="primitives"
      title="3 · Primitives"
      subtitle="@agentdash/ui 当前导出的 primitive；标 [TODO] 的为本任务待新增，此页面用临时模拟版展示形态。"
    >
      <div className="space-y-12">
        <PrimBadge />
        <PrimOriginBadge />
        <PrimStatusDot />
        <PrimInspectorRow />
        <PrimSectionTitle />
        <PrimButton />
        <PrimCreateButton />
        <PrimCard />
        <PrimNotice />
        <PrimEmptyState />
        <PrimField />
        <PrimFormControls />
        <PrimCardMenu />
        <PrimDetailPanel />
      </div>
    </SectionShell>
  );
}

function PrimSlot({
  name,
  importHint,
  todo = false,
  children,
}: {
  name: string;
  importHint: string;
  todo?: boolean;
  children: ReactNode;
}) {
  return (
    <article className="space-y-3">
      <div className="flex flex-wrap items-baseline justify-between gap-2 border-b border-border/40 pb-2">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-semibold">{name}</h3>
          {todo && <Badge variant="warning">TODO</Badge>}
        </div>
        <code className="font-mono text-[10px] text-muted-foreground">
          {importHint}
        </code>
      </div>
      <div className="rounded-[8px] border border-border/40 bg-card/30 p-4">
        {children}
      </div>
    </article>
  );
}

function PrimBadge() {
  return (
    <PrimSlot
      name="Badge"
      importHint='import { Badge } from "@agentdash/ui"'
    >
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="neutral">neutral</Badge>
        <Badge variant="primary">primary</Badge>
        <Badge variant="success">success · 已就绪</Badge>
        <Badge variant="warning">warning · 待审核</Badge>
        <Badge variant="danger">danger · 失败</Badge>
        <Badge variant="info">info</Badge>
        <Badge variant="accent">accent · 已发布</Badge>
      </div>
      <p className="mt-3 text-[11px] text-muted-foreground">
        本任务将扩 <code className="font-mono">info</code> /{" "}
        <code className="font-mono">accent</code> 两个 variant，并把
        SUPERVISED 权限策略 / PublishedBadge 等场景迁移过来。
      </p>
    </PrimSlot>
  );
}

function PrimOriginBadge() {
  const origins: OriginKind[] = [
    "builtin_seed",
    "user",
    "github",
    "clawhub",
    "skills_sh",
    "marketplace",
  ];
  return (
    <PrimSlot
      name="OriginBadge"
      todo
      importHint='import { OriginBadge } from "@agentdash/ui"'
    >
      <div className="flex flex-wrap items-center gap-2">
        {origins.map((o) => (
          <OriginBadgePreview key={o} origin={o} />
        ))}
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <OriginBadgePreview origin="github" subText="anthropic/claude-code" />
        <OriginBadgePreview origin="user" subText="my-skill v1.2.0" />
      </div>
    </PrimSlot>
  );
}

function PrimStatusDot() {
  const tones: DotTone[] = ["success", "warning", "danger", "info", "muted"];
  return (
    <PrimSlot
      name="StatusDot"
      todo
      importHint='import { StatusDot } from "@agentdash/ui"'
    >
      <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
        <div className="space-y-2">
          <p className="agentdash-form-label">size=sm</p>
          <div className="flex items-center gap-3">
            {tones.map((t) => (
              <span key={t} className="flex items-center gap-1.5 text-xs">
                <StatusDotPreview tone={t} title={t} />
                <span className="text-muted-foreground">{t}</span>
              </span>
            ))}
          </div>
        </div>
        <div className="space-y-2">
          <p className="agentdash-form-label">size=md</p>
          <div className="flex items-center gap-3">
            {tones.map((t) => (
              <span key={t} className="flex items-center gap-1.5 text-xs">
                <StatusDotPreview tone={t} size="md" title={t} />
                <span className="text-muted-foreground">{t}</span>
              </span>
            ))}
          </div>
        </div>
        <div className="space-y-2">
          <p className="agentdash-form-label">pulse</p>
          <div className="flex items-center gap-3">
            <span className="flex items-center gap-1.5 text-xs">
              <StatusDotPreview tone="success" pulse title="online" />
              <span className="text-muted-foreground">online</span>
            </span>
            <span className="flex items-center gap-1.5 text-xs">
              <StatusDotPreview tone="info" pulse title="connecting" />
              <span className="text-muted-foreground">connecting</span>
            </span>
          </div>
        </div>
      </div>
    </PrimSlot>
  );
}

function PrimInspectorRow() {
  return (
    <PrimSlot
      name="InspectorRow"
      todo
      importHint='import { InspectorRow } from "@agentdash/ui"'
    >
      <dl className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <InspectorRowPreview label="path" value="skills/ld-assistant/SKILL.md" mono />
        <InspectorRowPreview label="size" value="9.4 KB" />
        <InspectorRowPreview label="mode" value="readonly" tone="muted" />
        <InspectorRowPreview label="status" value="synced" tone="success" />
        <InspectorRowPreview label="error" value="checksum mismatch" tone="danger" />
        <InspectorRowPreview
          label="warning"
          value="包含未保存改动"
          tone="warning"
        />
      </dl>
    </PrimSlot>
  );
}

function PrimSectionTitle() {
  return (
    <PrimSlot
      name="SectionTitle"
      todo
      importHint='import { SectionTitle } from "@agentdash/ui"'
    >
      <div className="space-y-3">
        <div className="overflow-hidden rounded-[8px] border border-border bg-card">
          <SectionTitlePreview title="文件" subtitle="mount" />
          <div className="px-4 py-3 text-xs text-muted-foreground">
            默认 SectionTitle，无 actions 和 badge。
          </div>
        </div>
        <div className="overflow-hidden rounded-[8px] border border-border bg-card">
          <SectionTitlePreview
            title="YAML meta"
            subtitle="SKILL.md"
            actions={
              <Button size="sm" variant="primary">
                保存 meta
              </Button>
            }
          />
          <div className="px-4 py-3 text-xs text-muted-foreground">
            含 actions 区，按钮放右侧。
          </div>
        </div>
        <div className="overflow-hidden rounded-[8px] border border-border bg-card">
          <SectionTitlePreview
            title="文件"
            subtitle="skill_asset_fs"
            badge={<Badge variant="neutral">SKILL.md</Badge>}
          />
          <div className="px-4 py-3 text-xs text-muted-foreground">含 badge。</div>
        </div>
      </div>
    </PrimSlot>
  );
}

function PrimButton() {
  return (
    <PrimSlot
      name="Button"
      importHint='import { Button } from "@agentdash/ui"'
    >
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="primary">primary</Button>
          <Button variant="secondary">secondary</Button>
          <Button variant="danger">danger</Button>
          <Button variant="ghost">ghost</Button>
          <Button variant="primary" disabled>
            disabled
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="primary" size="sm">
            sm
          </Button>
          <Button variant="primary" size="md">
            md
          </Button>
          <Button variant="primary" size="icon">
            ✓
          </Button>
        </div>
        <p className="text-[11px] text-muted-foreground">
          注意：本任务 S3 会把 Button radius 从 10 → 8，预计观感更紧凑。
        </p>
      </div>
    </PrimSlot>
  );
}

function PrimCreateButton() {
  return (
    <PrimSlot
      name="CreateButton"
      importHint='import { CreateButton } from "@agentdash/ui"'
    >
      <div className="space-y-3">
        <p className="text-[11px] text-muted-foreground">
          统一的"创建实体"入口按钮，渲染为 <code>+ {"{entity}"}</code>。entity 使用英文实体名（Story / Skill / Workflow / Preset / Routine / Project / Agent）。
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <CreateButton entity="Story" />
          <CreateButton entity="Skill" />
          <CreateButton entity="Workflow" />
          <CreateButton entity="Preset" />
          <CreateButton entity="Routine" />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <CreateButton entity="Project" variant="secondary" />
          <CreateButton entity="Agent" variant="ghost" />
          <CreateButton entity="Story" disabled />
        </div>
        <p className="text-[11px] text-muted-foreground">
          使用规约：列表/资产页头部主入口、空状态引导按钮统一用 <code>CreateButton</code>。
          <br />
          Dialog/Drawer footer 的"提交创建"按钮属于表单确认，仍用 <code>Button variant="primary"</code>，文案保留"创建 X"。
          <br />
          Kanban 列内联快加（如 Story Board 的 <code>+ Create in this column</code> 文本按钮、column 头部纯 <code>+</code> 触发器）保留各自形态。
        </p>
      </div>
    </PrimSlot>
  );
}

function PrimCard() {
  return (
    <PrimSlot
      name="Card / CardHeader"
      importHint='import { Card, CardHeader } from "@agentdash/ui"'
    >
      <div className="space-y-3">
        <Card>
          <CardHeader actions={<Button size="sm">操作</Button>}>
            <p className="text-sm font-semibold">默认 Card</p>
            <p className="text-xs text-muted-foreground">as=section · depth-1 容器</p>
          </CardHeader>
          <p className="text-sm text-muted-foreground">
            内部应使用 fieldset / space-y 分组，**不**再嵌套 border + bg
            的子卡片。
          </p>
        </Card>
        <Card as="article">
          <p className="text-sm font-semibold">as=article 卡片</p>
          <p className="text-xs text-muted-foreground">用于列表中的可点击卡片。</p>
        </Card>
      </div>
    </PrimSlot>
  );
}

function PrimNotice() {
  return (
    <PrimSlot
      name="Notice"
      importHint='import { Notice } from "@agentdash/ui"'
    >
      <div className="space-y-2">
        <Notice tone="info">info · 一般信息提示</Notice>
        <Notice tone="success">success · 操作成功</Notice>
        <Notice tone="warning">warning · 可能影响后续行为</Notice>
        <Notice tone="danger">danger · 操作失败或风险提示</Notice>
      </div>
    </PrimSlot>
  );
}

function PrimEmptyState() {
  return (
    <PrimSlot
      name="EmptyState"
      importHint='import { EmptyState } from "@agentdash/ui"'
    >
      <EmptyState>暂无 Skill 资产 · 点击「新建 Skill」开始</EmptyState>
    </PrimSlot>
  );
}

function PrimField() {
  return (
    <PrimSlot
      name="Field"
      importHint='import { Field } from "@agentdash/ui"'
    >
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <Field label="display name">
          <TextInput defaultValue="My Skill" />
        </Field>
        <Field label="key">
          <TextInput defaultValue="my-skill" />
        </Field>
      </div>
    </PrimSlot>
  );
}

function PrimFormControls() {
  return (
    <PrimSlot
      name="TextInput / Textarea / Select / CheckboxField"
      importHint='import { TextInput, Textarea, Select, CheckboxField } from "@agentdash/ui"'
    >
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <Field label="text input">
          <TextInput placeholder="placeholder" />
        </Field>
        <Field label="select">
          <Select defaultValue="builtin">
            <option value="builtin">builtin</option>
            <option value="user">user</option>
            <option value="github">github</option>
          </Select>
        </Field>
        <Field label="textarea">
          <Textarea placeholder="多行文本" rows={3} />
        </Field>
        <div className="flex items-end">
          <CheckboxField label="disable model invocation" />
        </div>
        <Field label="disabled">
          <TextInput disabled defaultValue="disabled" />
        </Field>
        <Field label="invalid (border-destructive 手动叠加)">
          <TextInput
            defaultValue="value"
            className="border-destructive focus:border-destructive focus:ring-destructive/30"
          />
        </Field>
      </div>
    </PrimSlot>
  );
}

// ────────────────────────────────────────────────────────
// Section 4 · Surface depth demo
// ────────────────────────────────────────────────────────

function SectionSurface() {
  return (
    <SectionShell
      id="surface"
      title="4 · Surface depth"
      subtitle="depth-1 容器内最多再有一层 depth-2，且 depth-2 只能选 border-t 或 bg-tinted 一种视觉提示。"
    >
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <SurfaceExample
          tag="depth-1"
          legal
          desc="单层壳：bg-card + border + rounded-md"
        >
          <Card>
            <CardHeader>
              <p className="text-sm font-semibold">YAML meta</p>
            </CardHeader>
            <div className="space-y-2 text-sm">
              <p>name: my-skill</p>
              <p>description: 一段描述</p>
            </div>
          </Card>
        </SurfaceExample>

        <SurfaceExample
          tag="depth-2 · border-t"
          legal
          desc="壳内仅用 border-t 分组，不叠 bg"
        >
          <Card>
            <CardHeader>
              <p className="text-sm font-semibold">Section title</p>
            </CardHeader>
            <div className="space-y-2 pb-3 text-sm">
              <p>顶部分区内容。</p>
            </div>
            <div className="space-y-2 border-t border-border/40 pt-3 text-sm">
              <p>底部分区内容（depth-2，只用 border-t）。</p>
            </div>
          </Card>
        </SurfaceExample>

        <SurfaceExample
          tag="错误反例"
          legal={false}
          desc="嵌套子卡片再叠 border + bg + rounded（违反二选一规则）"
        >
          <Card>
            <CardHeader>
              <p className="text-sm font-semibold">违反示例</p>
            </CardHeader>
            <div className="rounded-[8px] border border-border bg-secondary/40 p-3">
              <div className="rounded-[6px] border border-border bg-background px-3 py-2 text-xs">
                又一层盒子（已经 depth-3）
              </div>
            </div>
          </Card>
        </SurfaceExample>
      </div>
    </SectionShell>
  );
}

function SurfaceExample({
  tag,
  legal,
  desc,
  children,
}: {
  tag: string;
  legal: boolean;
  desc: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Badge variant={legal ? "success" : "danger"}>
          {legal ? "✓ 合法" : "✗ 反例"}
        </Badge>
        <code className="font-mono text-[10px] text-muted-foreground">{tag}</code>
      </div>
      <div>{children}</div>
      <p className="text-[11px] text-muted-foreground">{desc}</p>
    </div>
  );
}

// ────────────────────────────────────────────────────────
// Section 5 · Elevation & Shell
// ────────────────────────────────────────────────────────

const ELEVATIONS = [
  { name: "shadow-sm", cls: "shadow-sm", desc: "卡片静置 / sidebar 收边" },
  { name: "shadow-md", cls: "shadow-md", desc: "popover / dropdown 浮起" },
  { name: "shadow-lg", cls: "shadow-lg", desc: "dialog / 抽屉" },
] as const;

function SectionElevationShell() {
  return (
    <SectionShell
      id="elevation"
      title="5 · Elevation & Shell"
      subtitle="阴影是 depth 的第二信号，与 bg/border 协同；同一容器里阴影与重边框二选一。sidebar 为独立表面，与内容区(depth-0)拉开。"
    >
      <div className="space-y-8">
        {/* Elevation 三档 */}
        <div>
          <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            Elevation tokens
          </p>
          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            {ELEVATIONS.map((e) => (
              <div key={e.name} className="space-y-2">
                <div
                  className={cn(
                    "flex h-24 items-center justify-center rounded-[8px] bg-card",
                    e.cls,
                  )}
                >
                  <code className="font-mono text-xs text-muted-foreground">
                    {e.name}
                  </code>
                </div>
                <p className="text-[11px] text-muted-foreground">{e.desc}</p>
              </div>
            ))}
          </div>
        </div>

        {/* Sidebar 表面 vs 内容区表面：只演示两个底色层级的差异，非真实导航。 */}
        <div>
          <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            表面对比：--sidebar vs --background
          </p>
          <div className="flex h-40 overflow-hidden rounded-[12px] border border-border">
            <div className="grid w-44 place-items-center bg-sidebar p-3 shadow-sm">
              <code className="font-mono text-[11px] text-muted-foreground">--sidebar</code>
            </div>
            <div className="grid flex-1 place-items-center bg-background p-4">
              <Card className="shadow-sm">
                <p className="text-sm font-semibold">--background 上的卡片</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  内容区 depth-0；卡片 bg-card + shadow-sm；sidebar 用更冷的 --sidebar + 右缘阴影收边。
                </p>
              </Card>
            </div>
          </div>
        </div>

        {/* StatusScreen 全屏态 */}
        <div>
          <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            StatusScreen（splash / loading / 错误屏共用）
          </p>
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div className="h-56 overflow-hidden rounded-[12px] border border-border">
              <StatusScreen tone="loading" title="正在启动本机服务…" description="桌面端 API 就绪后进入工作台" />
            </div>
            <div className="h-56 overflow-hidden rounded-[12px] border border-border">
              <StatusScreen
                tone="danger"
                title="应用遇到错误"
                description="渲染时发生异常，可尝试重载。"
                action={
                  <Button variant="primary" size="sm">
                    重载应用
                  </Button>
                }
              />
            </div>
          </div>
        </div>
      </div>
    </SectionShell>
  );
}

// ────────────────────────────────────────────────────────
// Section 6 · 嵌套对比
// ────────────────────────────────────────────────────────

function SectionNestingCompare() {
  return (
    <SectionShell
      id="nesting"
      title="6 · 嵌套对比"
      subtitle="左：SkillVfsInspector 旧版 4 层嵌套（已经在调研期就地修过）；右：本任务交付的扁平化版本。"
    >
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <NestingOld />
        <NestingNew />
      </div>
    </SectionShell>
  );
}

function NestingOld() {
  return (
    <div className="space-y-2">
      <Badge variant="warning">旧版 · 4 层嵌套</Badge>
      <div className="overflow-hidden rounded-[8px] border border-border bg-secondary/10">
        <aside className="space-y-4 p-4">
          <header className="flex items-center justify-between gap-3">
            <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              YAML meta
            </h4>
            <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
              SKILL.md
            </span>
          </header>
          <section className="space-y-3 rounded-[8px] border border-border bg-background p-3">
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">name</span>
              <input
                value="ld-assistant"
                readOnly
                className="agentdash-form-input font-mono text-[12px] opacity-80"
              />
            </label>
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">description</span>
              <textarea
                value="LD-DesignerAssistant 是纯粹的分发层"
                readOnly
                className="agentdash-form-textarea min-h-16"
                rows={3}
              />
            </label>
            <label className="flex items-center gap-2 rounded-[8px] border border-border bg-secondary/20 px-3 py-2">
              <input type="checkbox" disabled />
              <span className="text-xs text-foreground">
                disable-model-invocation
              </span>
            </label>
            <div className="flex items-center justify-between gap-2 border-t border-border/70 pt-3">
              <span className="text-[10px] text-muted-foreground">已同步</span>
              <button
                disabled
                className="rounded-[6px] border border-success/30 bg-success/10 px-2 py-1 text-[11px] text-success opacity-50"
              >
                保存 meta
              </button>
            </div>
          </section>
        </aside>
      </div>
      <p className="text-[11px] text-muted-foreground">
        Panel(secondary/10) → section(border+bg-bg) → label(border+bg-secondary)
        → input[border]，最深 4 层。
      </p>
    </div>
  );
}

function NestingNew() {
  return (
    <div className="space-y-2">
      <Badge variant="success">新版 · 扁平 + sticky 顶栏</Badge>
      <div className="overflow-hidden rounded-[8px] border border-border bg-secondary/10">
        <aside className="flex flex-col">
          <SectionTitlePreview
            sticky
            title="YAML meta"
            subtitle="SKILL.md"
            actions={
              <Button size="sm" variant="primary">
                保存 meta
              </Button>
            }
          />
          <div className="space-y-5 px-4 py-4">
            <div className="space-y-3">
              <Field label="name">
                <TextInput
                  defaultValue="ld-assistant"
                  readOnly
                  className="font-mono text-[12px] opacity-80"
                />
              </Field>
              <Field label="description">
                <Textarea
                  defaultValue="LD-DesignerAssistant 是纯粹的分发层"
                  readOnly
                  rows={3}
                />
              </Field>
              <CheckboxField label="disable-model-invocation" disabled />
            </div>
          </div>
        </aside>
      </div>
      <p className="text-[11px] text-muted-foreground">
        Panel(secondary/10) → space-y-5 / Field 间距分组 → input[border]，最深 2
        层。无 row-box、无 pre-double-bg。
      </p>
    </div>
  );
}

// ────────────────────────────────────────────────────────
// Section 6 · Form 综合
// ────────────────────────────────────────────────────────

function SectionFormComposite() {
  const [showDialog, setShowDialog] = useState(false);
  return (
    <SectionShell
      id="form"
      title="7 · Form 综合"
      subtitle="模拟 Skill 编辑表单 + Dialog 嵌套，主要用于验收 input/button radius=8 后的整体观感。"
    >
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <p className="text-sm font-semibold">编辑 Skill</p>
            <p className="text-xs text-muted-foreground">skills/my-skill/SKILL.md</p>
          </CardHeader>
          <div className="space-y-3">
            <Field label="display name">
              <TextInput defaultValue="My Skill" />
            </Field>
            <Field label="key">
              <TextInput defaultValue="my-skill" />
            </Field>
            <Field label="description">
              <Textarea
                defaultValue="一段描述"
                rows={3}
              />
            </Field>
            <CheckboxField label="disable model invocation" />
            <div className="flex justify-end gap-2 border-t border-border/40 pt-3">
              <Button variant="secondary">取消</Button>
              <Button variant="primary">保存</Button>
            </div>
          </div>
        </Card>

        <div className="space-y-3">
          <Card>
            <CardHeader actions={<OriginBadgePreview origin="github" />}>
              <p className="text-sm font-semibold">列表卡片示例</p>
              <p className="text-xs text-muted-foreground">
                skills/ld-assistant/SKILL.md
              </p>
            </CardHeader>
            <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">
              LD-DesignerAssistant 是纯粹的分发层，只负责理解、拆分、分类、派遣。
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-1.5">
              <Badge variant="neutral">3 files</Badge>
              <Badge variant="warning">explicit only</Badge>
              <Badge variant="neutral">imported</Badge>
              <StatusDotPreview tone="success" pulse title="active" />
            </div>
          </Card>

          <Button variant="primary" onClick={() => setShowDialog(true)}>
            打开 Dialog 嵌套示例
          </Button>
          {showDialog && <FormDialog onClose={() => setShowDialog(false)} />}
        </div>
      </div>
    </SectionShell>
  );
}

function FormDialog({ onClose }: { onClose: () => void }) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        className="w-full max-w-md rounded-[12px] border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between border-b border-border px-5 py-4">
          <p className="text-sm font-semibold">嵌套 Dialog 示例</p>
          <Button variant="ghost" size="sm" onClick={onClose}>
            关闭
          </Button>
        </header>
        <div className="space-y-3 px-5 py-4">
          <Notice tone="info">外层 Dialog = depth-1 (radius lg=12)。</Notice>
          <Field label="display name">
            <TextInput defaultValue="My Skill" />
          </Field>
          <Field label="description">
            <Textarea defaultValue="一段描述" rows={2} />
          </Field>
        </div>
        <footer className="flex justify-end gap-2 border-t border-border px-5 py-3">
          <Button variant="secondary" onClick={onClose}>
            取消
          </Button>
          <Button variant="primary" onClick={onClose}>
            保存
          </Button>
        </footer>
      </div>
    </div>
  );
}

// ────────────────────────────────────────────────────────
// CardMenu / DetailPanel previews
// ────────────────────────────────────────────────────────

function PrimCardMenu() {
  return (
    <PrimSlot
      name="CardMenu"
      importHint='import { CardMenu } from "@agentdash/ui"'
    >
      <div className="flex flex-wrap items-center gap-3">
        <CardMenu
          items={[
            { key: "edit", label: "编辑", onSelect: () => {} },
            { key: "duplicate", label: "复制为副本", badge: "user", onSelect: () => {} },
            { key: "---", label: "", onSelect: () => {} },
            { key: "delete", label: "删除", danger: true, onSelect: () => {} },
          ]}
        />
        <span className="text-[11px] text-muted-foreground">点击三点图标查看下拉</span>
      </div>
    </PrimSlot>
  );
}

function PrimDetailPanel() {
  const [panelOpen, setPanelOpen] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [promptOpen, setPromptOpen] = useState(false);
  const [promptValue, setPromptValue] = useState("references/notes.md");
  const [dangerOpen, setDangerOpen] = useState(false);
  const [confirmInput, setConfirmInput] = useState("");

  return (
    <PrimSlot
      name="DetailPanel / DetailSection / DetailMenu / ConfirmDialog / PromptDialog / DangerConfirmDialog"
      importHint='import { DetailPanel, DetailSection, DetailMenu, ConfirmDialog, PromptDialog, DangerConfirmDialog } from "@agentdash/ui"'
    >
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="secondary" size="sm" onClick={() => setPanelOpen(true)}>
          打开 DetailPanel
        </Button>
        <Button variant="secondary" size="sm" onClick={() => setConfirmOpen(true)}>
          打开 ConfirmDialog
        </Button>
        <Button variant="secondary" size="sm" onClick={() => setPromptOpen(true)}>
          打开 PromptDialog
        </Button>
        <Button variant="danger" size="sm" onClick={() => setDangerOpen(true)}>
          打开 DangerConfirmDialog
        </Button>
        <DetailMenu
          items={[
            { key: "rename", label: "重命名", onSelect: () => {} },
            { key: "archive", label: "归档", onSelect: () => {} },
            { key: "remove", label: "删除", danger: true, onSelect: () => {} },
          ]}
        />
        <span className="text-[11px] text-muted-foreground">右侧三点为 DetailMenu 触发器</span>
      </div>

      <DetailPanel
        open={panelOpen}
        onClose={() => setPanelOpen(false)}
        title="DetailPanel 示例"
        subtitle="右侧抽屉容器，承载详情/编辑表单。"
      >
        <div className="space-y-4 p-5">
          <DetailSection title="基础信息" description="使用默认 padding 的 section。">
            <p className="text-sm text-muted-foreground">这里放表单或只读字段。</p>
          </DetailSection>
          <DetailSection title="紧凑模式" compact extra={<Badge variant="info">compact</Badge>}>
            <p className="text-xs text-muted-foreground">用于窄容器（DAG 侧栏 w-96）。</p>
          </DetailSection>
        </div>
      </DetailPanel>

      <ConfirmDialog
        open={confirmOpen}
        title="确认切换"
        description="此弹窗用于普通确认或轻量危险操作，由调用方决定确认后的业务动作。"
        confirmLabel="确认"
        onClose={() => setConfirmOpen(false)}
        onConfirm={() => setConfirmOpen(false)}
      />

      <PromptDialog
        open={promptOpen}
        title="输入文件路径"
        description="用于路径、名称等短文本输入。"
        label="文件路径"
        value={promptValue}
        confirmLabel="确认"
        onValueChange={setPromptValue}
        onClose={() => setPromptOpen(false)}
        onConfirm={() => setPromptOpen(false)}
      />

      <DangerConfirmDialog
        open={dangerOpen}
        title="确认删除"
        description="此操作不可撤销。请输入名称以确认。"
        expectedValue="DELETE"
        inputValue={confirmInput}
        onInputValueChange={setConfirmInput}
        confirmLabel="删除"
        onClose={() => {
          setDangerOpen(false);
          setConfirmInput("");
        }}
        onConfirm={() => {
          setDangerOpen(false);
          setConfirmInput("");
        }}
      />
    </PrimSlot>
  );
}

// ────────────────────────────────────────────────────────
// SectionShell helper
// ────────────────────────────────────────────────────────

function SectionShell({
  id,
  title,
  subtitle,
  children,
}: {
  id: string;
  title: string;
  subtitle: string;
  children: ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-20 space-y-4">
      <header className="space-y-1">
        <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
        <p className="max-w-3xl text-sm text-muted-foreground">{subtitle}</p>
      </header>
      {children}
    </section>
  );
}
