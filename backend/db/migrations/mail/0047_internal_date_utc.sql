-- `internal_date` is compared and sorted as text: the message lists order by
-- it and page with `internal_date < cursor`. IMAP servers report it in their
-- own time zone, so rows were stored with mixed offsets (`+02:00`, `+01:00`,
-- `+00:00`) and a message from a UTC-reporting server sorted one or two hours
-- below its real position, next to older mail. Rewrite every row as UTC in
-- one fixed shape; the sync writes that shape from now on.
UPDATE messages
SET internal_date = strftime('%Y-%m-%dT%H:%M:%S', internal_date) || '+00:00'
WHERE internal_date NOT GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]+00:00'
  AND strftime('%Y-%m-%dT%H:%M:%S', internal_date) IS NOT NULL;
