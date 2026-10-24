-- Schema Bridge: export the STRUCTURE of this database in one file.
-- Read-only: it only reads the system catalogue, never table contents.
-- pgAdmin: run it (F5), then click "Save results to file" and save as CSV.
SELECT 'column' AS record_type,
       n.nspname::text AS table_schema, c.relname::text AS table_name,
       CASE c.relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'partitioned table'
                      WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized view' END AS object_type,
       a.attnum::text AS ordinal_position, a.attname::text AS column_name,
       format_type(a.atttypid, a.atttypmod) AS data_type,
       (NOT a.attnotnull)::text AS is_nullable,
       pg_get_expr(d.adbin, d.adrelid) AS column_default,
       obj_description(c.oid, 'pg_class') AS table_comment,
       col_description(c.oid, a.attnum) AS column_comment,
       NULL::text AS constraint_name, NULL::text AS constraint_type, NULL::text AS definition,
       NULL::text AS ref_schema, NULL::text AS ref_table,
       NULL::text AS indexname, NULL::text AS indexdef,
       NULL::text AS estimated_rows, NULL::text AS triggers, 'postgresql' AS dbms
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
WHERE c.relkind IN ('r','p','v','m')
  AND n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%'
UNION ALL
SELECT 'constraint', n.nspname::text, c.relname::text, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       con.conname::text,
       CASE con.contype WHEN 'p' THEN 'PRIMARY KEY' WHEN 'f' THEN 'FOREIGN KEY'
            WHEN 'u' THEN 'UNIQUE' WHEN 'c' THEN 'CHECK' WHEN 'x' THEN 'EXCLUSION' END,
       pg_get_constraintdef(con.oid), rn.nspname::text, rc.relname::text,
       NULL, NULL, NULL, NULL, 'postgresql'
FROM pg_constraint con
JOIN pg_class c ON c.oid = con.conrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_class rc ON rc.oid = con.confrelid
LEFT JOIN pg_namespace rn ON rn.oid = rc.relnamespace
WHERE n.nspname NOT IN ('pg_catalog','information_schema') AND con.contype IN ('p','f','u','c','x')
UNION ALL
SELECT 'index', schemaname::text, tablename::text, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, NULL, NULL, indexname::text, indexdef, NULL, NULL, 'postgresql'
FROM pg_indexes
WHERE schemaname NOT IN ('pg_catalog','information_schema')
UNION ALL
SELECT 'table', n.nspname::text, c.relname::text, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       c.reltuples::bigint::text,
       (SELECT string_agg(tg.tgname, ', ') FROM pg_trigger tg WHERE tg.tgrelid = c.oid AND NOT tg.tgisinternal),
       'postgresql'
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r','p') AND n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%'
ORDER BY 1, 2, 3;
