import { useEffect, useMemo, useState } from "react";
import { fetchProjectSkillAssets } from "../../../services/skillAsset";
import type { SkillAssetDto } from "../../../types";
import type { CapabilityChip } from "./capability-picker";
import { CapabilityPicker } from "./capability-picker";

export function SkillAssetPicker({
  projectId,
  selectedKeys,
  onChange,
}: {
  projectId?: string;
  selectedKeys: string[];
  onChange: (keys: string[]) => void;
}) {
  const [skills, setSkills] = useState<SkillAssetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadSkills = async () => {
    if (!projectId) return;
    setIsLoading(true);
    setError(null);
    try {
      setSkills(await fetchProjectSkillAssets(projectId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "加载 Skill 资产失败");
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void loadSkills();
    // loadSkills 本身只依赖 projectId，把它纳入 deps 会因每次渲染重建函数引用导致无限 fetch。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  const toggleKey = (key: string) => {
    if (selectedKeys.includes(key)) {
      onChange(selectedKeys.filter((item) => item !== key));
      return;
    }
    onChange([...selectedKeys, key]);
  };

  const sortedSkills = useMemo(
    () => skills.slice().sort((a, b) => a.display_name.localeCompare(b.display_name, "zh-CN")),
    [skills],
  );

  const sourceLabel = (source: SkillAssetDto["source"]) => {
    if (source === "builtin_seed") return "builtin";
    if (source === "github") return "github";
    return "user";
  };

  return (
    <CapabilityPicker
      hint={<>选中的项目 Skill 会以 <span className="font-mono">skills/&lt;key&gt;/SKILL.md</span> 注入会话 VFS。</>}
      isLoading={isLoading}
      error={error}
      items={sortedSkills}
      selectedKeys={selectedKeys}
      itemKey={(s) => s.key}
      itemToCardProps={(s) => {
        const chips: CapabilityChip[] = [{ label: sourceLabel(s.source) }];
        if (s.disable_model_invocation) chips.push({ label: 'explicit', variant: 'warning' });
        return {
          reactKey: s.id,
          title: s.display_name,
          subtitle: s.key,
          description: s.description?.trim() || undefined,
          chips,
        };
      }}
      onToggle={toggleKey}
      loadingText="正在加载 Skill…"
      emptyAllText="当前项目还没有 Skill 资产"
      enabledEmptyText="尚未启用任何 Skill，从下方选取。"
      availableEmptyText="所有 Skill 都已启用。"
    />
  );
}
