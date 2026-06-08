-- Add a published visibility tier for workspace tools.

ALTER TABLE workspace_tools
    DROP CONSTRAINT IF EXISTS workspace_tools_scope_visibility_check;

ALTER TABLE workspace_tools
    ADD CONSTRAINT workspace_tools_scope_visibility_check
        CHECK (scope_visibility IN ('private', 'workspace', 'published'));

ALTER TABLE workspace_tool_versions
    DROP CONSTRAINT IF EXISTS workspace_tool_versions_scope_visibility_check;

ALTER TABLE workspace_tool_versions
    ADD CONSTRAINT workspace_tool_versions_scope_visibility_check
        CHECK (scope_visibility IN ('private', 'workspace', 'published'));
