// ─── Skill Asset ──────────────────────────────────────
//
// 对齐后端 DTO：crates/agentdash-api/src/dto/skill_asset.rs

import type { InstalledAssetSourceDto } from "./shared-library";

export type SkillAssetSource = "builtin_seed" | "user" | "github";

export interface SkillAssetFileDto {
  path: string;
  content?: string | null;
  content_kind: "text" | "binary" | string;
  mime_type?: string | null;
  size_bytes: number;
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
  remote_source?: RemoteSkillAssetSourceDto | null;
  installed_source?: InstalledAssetSourceDto | null;
  disable_model_invocation: boolean;
  files: SkillAssetFileDto[];
  created_at: string;
  updated_at: string;
}

export interface RemoteSkillAssetSourceDto {
  source_type: "github" | string;
  url: string;
  imported_at: string;
  digest: string;
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

export interface ImportRemoteSkillAssetRequest {
  url: string;
}

export interface ListSkillAssetQuery {
  source?: SkillAssetSource;
}
