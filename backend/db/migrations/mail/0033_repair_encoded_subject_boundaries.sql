-- mailparse versions used by the IMAP ENVELOPE path decoded adjacent RFC 2047
-- words without consuming their boundary. The payload was already decoded, but
-- a marker such as `?==?utf-8?q?` remained visible in subjects and snippets.
-- Repair existing UTF-8 rows; the sync decoder handles all charsets going
-- forward and refreshes these fields whenever a message is fetched again.

UPDATE messages
SET subject = REPLACE(subject, '?==?utf-8?q?', ''),
    subject_normalized = REPLACE(subject_normalized, '?==?utf-8?q?', ''),
    snippet = REPLACE(snippet, '?==?utf-8?q?', '')
WHERE INSTR(subject, '?==?utf-8?q?') > 0
   OR INSTR(subject_normalized, '?==?utf-8?q?') > 0
   OR INSTR(snippet, '?==?utf-8?q?') > 0;

UPDATE messages
SET subject = REPLACE(subject, '?==?UTF-8?Q?', ''),
    subject_normalized = REPLACE(subject_normalized, '?==?UTF-8?Q?', ''),
    snippet = REPLACE(snippet, '?==?UTF-8?Q?', '')
WHERE INSTR(subject, '?==?UTF-8?Q?') > 0
   OR INSTR(subject_normalized, '?==?UTF-8?Q?') > 0
   OR INSTR(snippet, '?==?UTF-8?Q?') > 0;

UPDATE messages
SET subject = REPLACE(subject, '?==?utf-8?b?', ''),
    subject_normalized = REPLACE(subject_normalized, '?==?utf-8?b?', ''),
    snippet = REPLACE(snippet, '?==?utf-8?b?', '')
WHERE INSTR(subject, '?==?utf-8?b?') > 0
   OR INSTR(subject_normalized, '?==?utf-8?b?') > 0
   OR INSTR(snippet, '?==?utf-8?b?') > 0;

UPDATE messages
SET subject = REPLACE(subject, '?==?UTF-8?B?', ''),
    subject_normalized = REPLACE(subject_normalized, '?==?UTF-8?B?', ''),
    snippet = REPLACE(snippet, '?==?UTF-8?B?', '')
WHERE INSTR(subject, '?==?UTF-8?B?') > 0
   OR INSTR(subject_normalized, '?==?UTF-8?B?') > 0
   OR INSTR(snippet, '?==?UTF-8?B?') > 0;
