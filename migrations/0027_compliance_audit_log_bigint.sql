ALTER TABLE compliance_audit_log
    ALTER COLUMN memories_purged   TYPE BIGINT,
    ALTER COLUMN raw_events_purged TYPE BIGINT;
