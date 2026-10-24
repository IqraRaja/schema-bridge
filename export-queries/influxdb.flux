// Schema Bridge time-series schema export for InfluxDB 2.x (and 1.8 with Flux enabled).
// Produces the normalised Schema Bridge export (record_type, bucket, retention,
// measurement, key, data_type, series, dbms); see docs/import-formats.md.
// Read-only: it reads bucket metadata and the last point of each series. It writes nothing.
// Not yet run on a real server. Field types use the "types" package (InfluxDB 2.2 or newer).
// Set the bucket name, run the script, and save the result as CSV.
import "influxdata/influxdb/schema"
import "types"

bucket = "plant"

bucketRow = buckets()
    |> filter(fn: (r) => r.name == bucket)
    |> map(fn: (r) => ({
        record_type: "bucket", bucket: r.name,
        retention: if exists r.retentionPeriod and r.retentionPeriod > 0
            then string(v: duration(v: r.retentionPeriod)) else "INF",
        measurement: "", key: "", data_type: "", series: "", dbms: "influxdb",
    }))

measurementRows = schema.measurements(bucket: bucket)
    |> map(fn: (r) => ({
        record_type: "measurement", bucket: bucket, retention: "",
        measurement: r._value, key: "", data_type: "", series: "", dbms: "influxdb",
    }))

// The last point of every series and field.
latest = from(bucket: bucket)
    |> range(start: 0)
    |> last()

tagRows = latest
    |> keys(except: ["_start", "_stop", "_field", "_measurement", "_time", "_value"])
    |> map(fn: (r) => ({
        record_type: "tag", bucket: bucket, retention: "",
        measurement: r._measurement, key: r._value, data_type: "", series: "", dbms: "influxdb",
    }))
    |> group(columns: ["measurement", "key"])
    |> first(column: "record_type")

fieldRows = latest
    |> map(fn: (r) => ({
        record_type: "field", bucket: bucket, retention: "",
        measurement: r._measurement, key: r._field,
        data_type: if types.isType(v: r._value, type: "float") then "float"
            else if types.isType(v: r._value, type: "int") then "integer"
            else if types.isType(v: r._value, type: "uint") then "unsigned"
            else if types.isType(v: r._value, type: "bool") then "boolean"
            else if types.isType(v: r._value, type: "string") then "string"
            else "",
        series: "", dbms: "influxdb",
    }))
    |> group(columns: ["measurement", "key", "data_type"])
    |> first(column: "record_type")

union(tables: [bucketRow, measurementRows, tagRows, fieldRows])
    |> group()
    |> keep(columns: ["record_type", "bucket", "retention", "measurement", "key", "data_type", "series", "dbms"])
