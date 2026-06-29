ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_prefix_len;

ALTER TABLE api_keys ALTER COLUMN prefix TYPE varchar(32);

ALTER TABLE api_keys
    ADD CONSTRAINT api_keys_prefix_len CHECK (char_length(prefix) IN (8, 13, 21));

ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_prefix_version_supported;

ALTER TABLE api_keys
    ADD CONSTRAINT api_keys_prefix_version_supported CHECK (prefix_version IN (1, 2, 3));

COMMENT ON COLUMN api_keys.prefix_version IS
    '1 = legacy 8-character prefix requiring rotation; 2 = 13-character workspace prefix; 3 = 21-character prefix with random entropy';