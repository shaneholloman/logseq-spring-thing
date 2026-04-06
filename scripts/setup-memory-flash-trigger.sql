-- Memory Flash: PG trigger that fires NOTIFY on every INSERT/UPDATE/DELETE
-- to the memory_entries table so the VisionFlow embedding cloud can animate
-- the affected point in real time.
--
-- Usage: psql "$RUVECTOR_PG_CONNINFO" -f scripts/setup-memory-flash-trigger.sql

-- 1. Notification function
CREATE OR REPLACE FUNCTION notify_memory_access() RETURNS trigger AS $$
DECLARE
    payload jsonb;
    entry_key text;
    entry_ns  text;
BEGIN
    IF TG_OP = 'DELETE' THEN
        entry_key := OLD.key;
        entry_ns  := COALESCE(OLD.namespace, '');
    ELSE
        entry_key := NEW.key;
        entry_ns  := COALESCE(NEW.namespace, '');
    END IF;

    payload := jsonb_build_object(
        'key',       entry_key,
        'namespace', entry_ns,
        'action',    lower(TG_OP),
        'ts',        extract(epoch FROM now())::bigint
    );

    PERFORM pg_notify('memory_access', payload::text);
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- 2. Drop existing trigger if present (idempotent)
DROP TRIGGER IF EXISTS trg_memory_access ON memory_entries;

-- 3. Create trigger for INSERT, UPDATE, DELETE
CREATE TRIGGER trg_memory_access
    AFTER INSERT OR UPDATE OR DELETE ON memory_entries
    FOR EACH ROW EXECUTE FUNCTION notify_memory_access();

-- Verify
SELECT 'trigger installed' AS status;
