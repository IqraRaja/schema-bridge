-- Schema Bridge export for Microsoft SQL Server 2017+ / Azure SQL.
-- Read-only: it only reads the system catalogue, never table contents.
-- SSMS: before saving, turn on Tools > Options > Query Results > SQL Server > Results to Grid >
--       "Include column headers when copying or saving the results" and
--       "Quote strings containing list separators when saving .csv results" (then open a new query window).
DECLARE @n nvarchar(max) = NULL;
SELECT CAST('column' AS nvarchar(20)) AS record_type, s.name AS table_schema, o.name AS table_name,
       CAST(CASE o.type WHEN 'V' THEN 'view' ELSE 'table' END AS nvarchar(20)) AS object_type,
       CAST(c.column_id AS nvarchar(10)) AS ordinal_position, c.name AS column_name,
       CAST(CASE WHEN ty.name IN ('varchar','char','varbinary','binary')
                   THEN ty.name + '(' + CASE WHEN c.max_length = -1 THEN 'max' ELSE CAST(c.max_length AS nvarchar(10)) END + ')'
                 WHEN ty.name IN ('nvarchar','nchar')
                   THEN ty.name + '(' + CASE WHEN c.max_length = -1 THEN 'max' ELSE CAST(c.max_length / 2 AS nvarchar(10)) END + ')'
                 WHEN ty.name IN ('decimal','numeric')
                   THEN ty.name + '(' + CAST(c.precision AS nvarchar(10)) + ',' + CAST(c.scale AS nvarchar(10)) + ')'
                 ELSE ty.name END AS nvarchar(200)) AS data_type,
       CAST(CASE WHEN c.is_nullable = 1 THEN 'true' ELSE 'false' END AS nvarchar(10)) AS is_nullable,
       CAST(COALESCE(dc.definition, CASE WHEN c.is_identity = 1 THEN 'identity' END) AS nvarchar(max)) AS column_default,
       CAST(tep.value AS nvarchar(max)) AS table_comment, CAST(cep.value AS nvarchar(max)) AS column_comment,
       @n AS constraint_name, @n AS constraint_type, @n AS definition, @n AS ref_schema, @n AS ref_table,
       @n AS indexname, @n AS indexdef, @n AS estimated_rows, @n AS triggers, CAST('sqlserver' AS nvarchar(20)) AS dbms
FROM sys.objects o
JOIN sys.schemas s ON s.schema_id = o.schema_id
JOIN sys.columns c ON c.object_id = o.object_id
JOIN sys.types ty ON ty.user_type_id = c.user_type_id
LEFT JOIN sys.default_constraints dc ON dc.object_id = c.default_object_id
LEFT JOIN sys.extended_properties tep ON tep.class = 1 AND tep.major_id = o.object_id AND tep.minor_id = 0 AND tep.name = 'MS_Description'
LEFT JOIN sys.extended_properties cep ON cep.class = 1 AND cep.major_id = o.object_id AND cep.minor_id = c.column_id AND cep.name = 'MS_Description'
WHERE o.type IN ('U','V') AND o.is_ms_shipped = 0
UNION ALL
SELECT 'constraint', s.name, t.name, @n, @n, @n, @n, @n, @n, @n, @n,
       kc.name, CASE kc.type WHEN 'PK' THEN 'PRIMARY KEY' ELSE 'UNIQUE' END,
       CASE kc.type WHEN 'PK' THEN 'PRIMARY KEY' ELSE 'UNIQUE' END + ' ('
         + (SELECT STRING_AGG(c.name, ', ') WITHIN GROUP (ORDER BY ic.key_ordinal)
              FROM sys.index_columns ic JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
             WHERE ic.object_id = kc.parent_object_id AND ic.index_id = kc.unique_index_id AND ic.is_included_column = 0) + ')',
       @n, @n, @n, @n, @n, @n, 'sqlserver'
FROM sys.key_constraints kc
JOIN sys.tables t ON t.object_id = kc.parent_object_id
JOIN sys.schemas s ON s.schema_id = t.schema_id
UNION ALL
SELECT 'constraint', s.name, t.name, @n, @n, @n, @n, @n, @n, @n, @n,
       fk.name, 'FOREIGN KEY',
       'FOREIGN KEY ('
         + (SELECT STRING_AGG(c.name, ', ') WITHIN GROUP (ORDER BY fkc.constraint_column_id)
              FROM sys.foreign_key_columns fkc JOIN sys.columns c ON c.object_id = fkc.parent_object_id AND c.column_id = fkc.parent_column_id
             WHERE fkc.constraint_object_id = fk.object_id)
         + ') REFERENCES ' + rs.name + '.' + rt.name + '('
         + (SELECT STRING_AGG(c.name, ', ') WITHIN GROUP (ORDER BY fkc.constraint_column_id)
              FROM sys.foreign_key_columns fkc JOIN sys.columns c ON c.object_id = fkc.referenced_object_id AND c.column_id = fkc.referenced_column_id
             WHERE fkc.constraint_object_id = fk.object_id) + ')'
         + CASE WHEN fk.delete_referential_action_desc <> 'NO_ACTION' THEN ' ON DELETE ' + REPLACE(fk.delete_referential_action_desc, '_', ' ') ELSE '' END
         + CASE WHEN fk.is_not_trusted = 1 OR fk.is_disabled = 1 THEN ' NOT VALID' ELSE '' END,
       rs.name, rt.name, @n, @n, @n, @n, 'sqlserver'
FROM sys.foreign_keys fk
JOIN sys.tables t ON t.object_id = fk.parent_object_id
JOIN sys.schemas s ON s.schema_id = t.schema_id
JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id
JOIN sys.schemas rs ON rs.schema_id = rt.schema_id
UNION ALL
SELECT 'constraint', s.name, t.name, @n, @n, @n, @n, @n, @n, @n, @n,
       cc.name, 'CHECK', 'CHECK ' + cc.definition, @n, @n, @n, @n, @n, @n, 'sqlserver'
FROM sys.check_constraints cc
JOIN sys.tables t ON t.object_id = cc.parent_object_id
JOIN sys.schemas s ON s.schema_id = t.schema_id
UNION ALL
SELECT 'index', s.name, t.name, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n,
       i.name,
       'CREATE ' + CASE WHEN i.is_unique = 1 THEN 'UNIQUE ' ELSE '' END + 'INDEX ' + i.name + ' ON ' + s.name + '.' + t.name + ' ('
         + COALESCE((SELECT STRING_AGG(c.name, ', ') WITHIN GROUP (ORDER BY ic.key_ordinal)
              FROM sys.index_columns ic JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id
             WHERE ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0), '') + ')'
         + CASE WHEN i.has_filter = 1 THEN ' WHERE ' + i.filter_definition ELSE '' END,
       @n, @n, 'sqlserver'
FROM sys.indexes i
JOIN sys.tables t ON t.object_id = i.object_id
JOIN sys.schemas s ON s.schema_id = t.schema_id
WHERE i.type > 0 AND i.is_hypothetical = 0 AND t.is_ms_shipped = 0
UNION ALL
SELECT 'table', s.name, t.name, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n, @n,
       CAST((SELECT SUM(p.rows) FROM sys.partitions p WHERE p.object_id = t.object_id AND p.index_id IN (0, 1)) AS nvarchar(30)),
       (SELECT STRING_AGG(tr.name, ', ') FROM sys.triggers tr WHERE tr.parent_id = t.object_id),
       'sqlserver'
FROM sys.tables t
JOIN sys.schemas s ON s.schema_id = t.schema_id
WHERE t.is_ms_shipped = 0
ORDER BY 1, 2, 3;
