-- Add a published visibility tier for workspace skills.

ALTER TABLE workspace_skills
    DROP CONSTRAINT IF EXISTS workspace_skills_scope_visibility_check;

ALTER TABLE workspace_skills
    ADD CONSTRAINT workspace_skills_scope_visibility_check
        CHECK (scope_visibility IN ('private', 'workspace', 'published'));

ALTER TABLE workspace_skill_versions
    DROP CONSTRAINT IF EXISTS workspace_skill_versions_scope_visibility_check;

ALTER TABLE workspace_skill_versions
    ADD CONSTRAINT workspace_skill_versions_scope_visibility_check
        CHECK (scope_visibility IN ('private', 'workspace', 'published'));