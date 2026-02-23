-- Fix admin password: set correct bcrypt hash for "admin123" (cost 12)
-- Run: Get-Content fix_admin_password.sql | docker exec -i fire-detect-db psql -U fire_detect -d fire_detect
UPDATE users
SET password_hash = '$2b$12$TA2cVfbUfATgBP9ZTTFubOBvHm9kfon1a8hvKf/PcgyiupX48H7VS'
WHERE email = 'admin@example.com';
