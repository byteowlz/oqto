-- Upgrade dev user to admin role
-- In dev mode, the developer user should have admin role for convenience
-- This allows access to admin dashboard and all features

UPDATE users 
SET role = 'admin', 
    updated_at = datetime('now')
WHERE id = 'usr_dev' AND role = 'user';
