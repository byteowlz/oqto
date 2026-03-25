-- Remove hardcoded dev users from the initial migration.
-- These were a security risk: default admin/admin and dev/dev credentials
-- existed in every database, including production deployments.
--
-- Users who need admin accounts should create them via:
--   oqtoctl user create <username> -e <email> -r admin
-- Or via the Docker entrypoint's ADMIN_PASSWORD environment variable.

DELETE FROM users WHERE id = 'usr_admin' AND username = 'admin' AND email = 'admin@localhost';
DELETE FROM users WHERE id = 'usr_dev' AND username = 'dev' AND email = 'dev@localhost';
