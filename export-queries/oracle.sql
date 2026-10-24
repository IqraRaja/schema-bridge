-- Schema Bridge export for Oracle 12c and newer.
-- Read-only: it only reads the ALL_* dictionary views, never table contents.
-- It exports the schema you are logged in to (or the one set with ALTER SESSION SET CURRENT_SCHEMA).
SELECT CAST('column' AS VARCHAR2(20)) AS record_type, c.owner AS table_schema, c.table_name AS table_name,
       CAST(CASE WHEN v.view_name IS NOT NULL THEN 'view' ELSE 'table' END AS VARCHAR2(20)) AS object_type,
       c.column_id AS ordinal_position, c.column_name AS column_name,
       CASE WHEN c.data_type IN ('VARCHAR2','NVARCHAR2','CHAR','NCHAR') THEN c.data_type || '(' || c.char_length || ')'
            WHEN c.data_type = 'RAW' THEN 'RAW(' || c.data_length || ')'
            WHEN c.data_type = 'NUMBER' AND c.data_precision IS NOT NULL THEN 'NUMBER(' || c.data_precision || ',' || NVL(c.data_scale, 0) || ')'
            ELSE c.data_type END AS data_type,
       CASE c.nullable WHEN 'Y' THEN 'true' ELSE 'false' END AS is_nullable,
       CASE WHEN c.identity_column = 'YES' THEN 'identity' WHEN c.default_length > 0 THEN 'has default' END AS column_default,
       tc.comments AS table_comment, cc.comments AS column_comment,
       NULL AS constraint_name, NULL AS constraint_type, NULL AS definition, NULL AS ref_schema, NULL AS ref_table,
       NULL AS indexname, NULL AS indexdef, NULL AS estimated_rows, NULL AS triggers, CAST('oracle' AS VARCHAR2(20)) AS dbms
FROM all_tab_columns c
LEFT JOIN all_views v ON v.owner = c.owner AND v.view_name = c.table_name
LEFT JOIN all_tab_comments tc ON tc.owner = c.owner AND tc.table_name = c.table_name
LEFT JOIN all_col_comments cc ON cc.owner = c.owner AND cc.table_name = c.table_name AND cc.column_name = c.column_name
WHERE c.owner = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AND c.table_name NOT LIKE 'BIN$%'
UNION ALL
SELECT 'constraint', con.owner, con.table_name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       con.constraint_name,
       CASE con.constraint_type WHEN 'P' THEN 'PRIMARY KEY' WHEN 'U' THEN 'UNIQUE' ELSE 'FOREIGN KEY' END,
       CASE con.constraint_type WHEN 'P' THEN 'PRIMARY KEY' WHEN 'U' THEN 'UNIQUE' ELSE 'FOREIGN KEY' END
         || ' (' || (SELECT LISTAGG(k.column_name, ', ') WITHIN GROUP (ORDER BY k.position)
                       FROM all_cons_columns k WHERE k.owner = con.owner AND k.constraint_name = con.constraint_name) || ')'
         || CASE WHEN con.constraint_type = 'R' THEN ' REFERENCES ' || r.owner || '.' || r.table_name || '('
              || (SELECT LISTAGG(rk.column_name, ', ') WITHIN GROUP (ORDER BY rk.position)
                    FROM all_cons_columns rk WHERE rk.owner = r.owner AND rk.constraint_name = r.constraint_name) || ')' END
         || CASE WHEN con.delete_rule IN ('CASCADE', 'SET NULL') THEN ' ON DELETE ' || con.delete_rule END
         || CASE WHEN con.status = 'DISABLED' OR con.validated = 'NOT VALIDATED' THEN ' NOT VALID' END,
       r.owner, r.table_name, NULL, NULL, NULL, NULL, 'oracle'
FROM all_constraints con
LEFT JOIN all_constraints r ON r.owner = con.r_owner AND r.constraint_name = con.r_constraint_name
WHERE con.owner = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AND con.constraint_type IN ('P', 'U', 'R')
  AND con.table_name NOT LIKE 'BIN$%'
UNION ALL
SELECT 'index', i.table_owner, i.table_name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, NULL, NULL, i.index_name,
       'CREATE ' || CASE WHEN i.uniqueness = 'UNIQUE' THEN 'UNIQUE ' END || 'INDEX ' || i.index_name || ' ON ' || i.table_name || ' ('
         || (SELECT LISTAGG(ic.column_name, ', ') WITHIN GROUP (ORDER BY ic.column_position)
               FROM all_ind_columns ic WHERE ic.index_owner = i.owner AND ic.index_name = i.index_name) || ')',
       NULL, NULL, 'oracle'
FROM all_indexes i
WHERE i.table_owner = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AND i.index_type <> 'LOB' AND i.table_name NOT LIKE 'BIN$%'
UNION ALL
SELECT 'table', t.owner, t.table_name, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
       NULL, NULL, NULL, NULL, NULL, NULL, NULL, TO_CHAR(t.num_rows),
       (SELECT LISTAGG(tr.trigger_name, ', ') WITHIN GROUP (ORDER BY tr.trigger_name)
          FROM all_triggers tr WHERE tr.table_owner = t.owner AND tr.table_name = t.table_name),
       'oracle'
FROM all_tables t
WHERE t.owner = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') AND t.table_name NOT LIKE 'BIN$%' AND t.nested = 'NO' AND t.secondary = 'N'
ORDER BY 1, 2, 3
