-- Schema Bridge export for SQLite 3.16 and newer.
-- Read-only: it only reads sqlite_master and PRAGMA information, never table contents.
SELECT 'column' AS record_type, 'main' AS table_schema, m.name AS table_name, m.type AS object_type,
       p.cid + 1 AS ordinal_position, p.name AS column_name, lower(p.type) AS data_type,
       CASE WHEN p."notnull" = 1 OR p.pk > 0 THEN 'false' ELSE 'true' END AS is_nullable,
       p.dflt_value AS column_default, NULL AS table_comment, NULL AS column_comment,
       NULL AS constraint_name, NULL AS constraint_type, NULL AS definition, NULL AS ref_schema, NULL AS ref_table,
       NULL AS indexname, NULL AS indexdef, NULL AS estimated_rows, NULL AS triggers, 'sqlite' AS dbms
FROM sqlite_master m JOIN pragma_table_info(m.name) p
WHERE m.type IN ('table', 'view') AND m.name NOT LIKE 'sqlite_%'
UNION ALL
SELECT 'constraint', 'main', m.name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       m.name || '_pkey', 'PRIMARY KEY',
       'PRIMARY KEY (' || (SELECT group_concat(name, ', ') FROM (SELECT name FROM pragma_table_info(m.name) WHERE pk > 0 ORDER BY pk)) || ')',
       NULL, NULL, NULL, NULL, NULL, NULL, 'sqlite'
FROM sqlite_master m
WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%' AND EXISTS (SELECT 1 FROM pragma_table_info(m.name) WHERE pk > 0)
UNION ALL
SELECT 'constraint', 'main', m.name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       m.name || '_fk' || f.id, 'FOREIGN KEY',
       'FOREIGN KEY (' || group_concat(f."from", ', ') || ') REFERENCES ' || f."table" || '(' || group_concat(COALESCE(f."to", ''), ', ') || ')'
         || CASE WHEN f.on_delete IN ('CASCADE', 'SET NULL', 'SET DEFAULT') THEN ' ON DELETE ' || f.on_delete ELSE '' END,
       'main', f."table", NULL, NULL, NULL, NULL, 'sqlite'
FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f
WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'
GROUP BY m.name, f.id
UNION ALL
SELECT 'index', 'main', m.name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, NULL, NULL, il.name,
       'CREATE ' || CASE WHEN il."unique" THEN 'UNIQUE ' ELSE '' END || 'INDEX ' || il.name || ' ON ' || m.name || ' ('
         || (SELECT group_concat(name, ', ') FROM (SELECT name FROM pragma_index_info(il.name) ORDER BY seqno)) || ')'
         || CASE WHEN il.partial THEN ' WHERE (partial)' ELSE '' END,
       NULL, NULL, 'sqlite'
FROM sqlite_master m JOIN pragma_index_list(m.name) il
WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'
UNION ALL
SELECT 'table', 'main', m.name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       (SELECT group_concat(t.name, ', ') FROM sqlite_master t WHERE t.type = 'trigger' AND t.tbl_name = m.name),
       'sqlite'
FROM sqlite_master m
WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'
ORDER BY 1, 2, 3;
