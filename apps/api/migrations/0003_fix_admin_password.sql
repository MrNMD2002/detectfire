-- Fix admin login: update password_hash to a bcrypt hash that verifies correctly (admin123, cost 12)
UPDATE users
SET password_hash = '$2b$12$TA2cVfbUfATgBP9ZTTFubOBvHm9kfon1a8hvKf/PcgyiupX48H7VS'
WHERE email = 'admin@example.com';
