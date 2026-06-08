CREATE UNIQUE INDEX IF NOT EXISTS idx_push_sub_user_endpoint ON push_subscriptions(user_id, endpoint);
