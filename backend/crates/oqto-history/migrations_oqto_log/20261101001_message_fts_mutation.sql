-- This is an ordinary contentful FTS5 table. The special INSERT 'delete'
-- command is for external/contentless tables and aborts mutations here.
-- Correct derived-index maintenance without changing canonical payloads.
DROP TRIGGER IF EXISTS oqto_log_messages_ad;
DROP TRIGGER IF EXISTS oqto_log_messages_au;

CREATE TRIGGER oqto_log_messages_ad
AFTER DELETE ON oqto_log_messages
BEGIN
    DELETE FROM oqto_log_message_fts WHERE rowid = OLD.rowid;
END;

CREATE TRIGGER oqto_log_messages_au
AFTER UPDATE ON oqto_log_messages
BEGIN
    DELETE FROM oqto_log_message_fts WHERE rowid = OLD.rowid;
    INSERT INTO oqto_log_message_fts (rowid, message_id, turn_id, session_id, role, content)
    SELECT NEW.rowid, NEW.message_id, NEW.turn_id,
           (SELECT session_id FROM oqto_log_turns WHERE turn_id = NEW.turn_id),
           COALESCE(NEW.role, ''), NEW.content
    WHERE NEW.content IS NOT NULL;
END;
