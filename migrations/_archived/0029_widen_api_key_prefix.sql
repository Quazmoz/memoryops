-- S7 widens newly generated API key prefixes from the legacy 8-character
-- value to the 13-character mops_XXXXXXXX segment. Existing version 1 keys
-- cannot be re-derived from key_hash and must be rotated.
ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_prefix_len;

ALTER TABLE api_keys ALTER COLUMN prefix TYPE varchar(16);

ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS prefix_version SMALLINT NOT NULL DEFAULT 1;

ALTER TABLE api_keys
    ADD CONSTRAINT api_keys_prefix_len CHECK (char_length(prefix) IN (8, 13));

ALTER TABLE api_keys
    ADD CONSTRAINT api_keys_prefix_version_supported CHECK (prefix_version IN (1, 2));

COMMENT ON COLUMN api_keys.prefix_version IS
    '1 = legacy 8-character prefix requiring rotation; 2 = widened 13-character prefix';
