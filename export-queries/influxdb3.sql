-- Schema Bridge time-series schema export for InfluxDB 3 (SQL).
-- Produces the normalised Schema Bridge export (record_type, bucket, retention,
-- measurement, key, data_type, series, dbms); see docs/import-formats.md.
-- Read-only: it only reads information_schema. Not yet run on a real server.
-- Replace 'plant' with your database name. Retention and series counts are
-- not available here, so those columns stay empty.
SELECT 'measurement' AS record_type, 'plant' AS bucket, '' AS retention,
       table_name AS measurement, '' AS key, '' AS data_type, '' AS series, 'influxdb' AS dbms
FROM information_schema.tables
WHERE table_schema = 'iox'
UNION ALL
SELECT CASE WHEN data_type LIKE 'Dictionary%' THEN 'tag' ELSE 'field' END,
       'plant', '', table_name, column_name,
       CASE data_type
           WHEN 'Float64' THEN 'float'
           WHEN 'Int64' THEN 'integer'
           WHEN 'UInt64' THEN 'unsigned'
           WHEN 'Utf8' THEN 'string'
           WHEN 'Boolean' THEN 'boolean'
           ELSE ''
       END,
       '', 'influxdb'
FROM information_schema.columns
WHERE table_schema = 'iox' AND column_name <> 'time'
ORDER BY 4, 1, 5;
