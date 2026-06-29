-- Migration: Rename Skills to Tools

-- 1) Rename main tables
ALTER TABLE workspace_skills RENAME TO workspace_tools;
ALTER TABLE workspace_skill_versions RENAME TO workspace_tool_versions;
ALTER TABLE workspace_skill_invocations RENAME TO workspace_tool_invocations;

-- 2) Rename columns on workspace_tool_versions
ALTER TABLE workspace_tool_versions RENAME COLUMN skill_id TO tool_id;

-- 3) Rename columns on workspace_tool_invocations
ALTER TABLE workspace_tool_invocations RENAME COLUMN skill_id TO tool_id;
ALTER TABLE workspace_tool_invocations RENAME COLUMN skill_name TO tool_name;
ALTER TABLE workspace_tool_invocations RENAME COLUMN skill_version TO tool_version;

-- 4) Recreate triggers
DROP TRIGGER IF EXISTS trg_workspace_skills_updated_at ON workspace_tools;
CREATE TRIGGER trg_workspace_tools_updated_at
    BEFORE UPDATE ON workspace_tools
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- 5) Rename indexes
ALTER INDEX IF EXISTS workspace_skills_workspace_id_enabled RENAME TO workspace_tools_workspace_id_enabled;
ALTER INDEX IF EXISTS workspace_skill_versions_workspace_name_version_idx RENAME TO workspace_tool_versions_workspace_name_version_idx;
ALTER INDEX IF EXISTS workspace_skill_invocations_skill_id_time_idx RENAME TO workspace_tool_invocations_tool_id_time_idx;
ALTER INDEX IF EXISTS workspace_skill_invocations_workspace_time_idx RENAME TO workspace_tool_invocations_workspace_time_idx;

-- 6) Add tool lifecycle audit actions to enum
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'tool_created';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'tool_updated';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'tool_deleted';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'tool_rolled_back';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'tool_invoked';
