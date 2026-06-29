CREATE TABLE IF NOT EXISTS app_admin_credentials (
    id                  BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    root_password_hash  TEXT NOT NULL,
    root_password_enc   TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER trg_app_admin_credentials_updated_at
    BEFORE UPDATE ON app_admin_credentials
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
