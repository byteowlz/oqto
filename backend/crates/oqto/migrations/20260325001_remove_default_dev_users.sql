-- Remove hardcoded dev users inserted by the initial migration (20241216001).
--
-- These were a security risk: default admin/admin and dev/dev credentials
-- with known bcrypt hashes existed in every database regardless of mode.
-- The initial migration can't be modified (sqlx checksums), so this
-- migration cleans up the mess for both existing and new installs.
--
-- Dev mode uses config-based dev_users (AuthConfig.dev_users), not DB users.
-- Production admin accounts are created via: oqtoctl user create <name> -r admin

DELETE FROM users WHERE id = 'usr_admin' AND username = 'admin' AND email = 'admin@localhost';
DELETE FROM users WHERE id = 'usr_dev' AND username = 'dev' AND email = 'dev@localhost';
