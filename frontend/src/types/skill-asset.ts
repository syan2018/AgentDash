// ─── Skill Asset ──────────────────────────────────────
//
// 对齐后端 DTO：crates/agentdash-api/src/dto/skill_asset.rs

export type SkillAssetSource = "builtin_seed" | "user";

export interface SkillAssetFileDto {
  path: string;
  content: string;
  kind?: "skill" | "reference" | "script" | "asset" | string | null;
}

export interface SkillAssetDto {
  id: string;
  project_id: string;
  key: string;
  display_name: string;
  description: string;
  source: SkillAssetSource;
  builtin_key?: string | null;
  disable_model_invocation: boolean;
  files: SkillAssetFileDto[];
  created_at: string;
  updated_at: string;
}

export interface CreateSkillAssetRequest {
  key: string;
  display_name: string;
  description: string;
  disable_model_invocation?: boolean;
  files: SkillAssetFileDto[];
}

export interface UpdateSkillAssetRequest {
  key?: string;
  display_name?: string;
  description?: string;
  disable_model_invocation?: boolean;
  files?: SkillAssetFileDto[];
}

export interface BootstrapSkillAssetRequest {
  builtin_key?: string;
}

export interface ListSkillAssetQuery {
  source?: SkillAssetSource;
}
