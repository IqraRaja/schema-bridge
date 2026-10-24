-- Made-up target database for the Schema Bridge time-series demo: "Readings",
-- a PostgreSQL database that should receive the plant's sensor data from
-- InfluxDB. Contains no real data.
--
-- demo/readings-postgresql.csv describes this schema in the one-file export
-- format. It was written by hand to match what export-queries/postgresql.sql
-- returns for these tables (not exported from a running server).

CREATE TABLE readings(
  id bigserial PRIMARY KEY,
  recorded_at timestamptz NOT NULL,
  site text NOT NULL,
  unit_id text NOT NULL,
  temperature double precision,
  pressure double precision,
  running boolean
);
-- Deliberately no unique rule on (site, unit_id, recorded_at).
CREATE INDEX readings_recorded_at_idx ON readings (recorded_at);
CREATE INDEX readings_site_unit_idx ON readings (site, unit_id);

CREATE TABLE pump_readings(
  id bigserial PRIMARY KEY,
  recorded_at timestamptz NOT NULL,
  site text NOT NULL,
  unit_id text NOT NULL,
  flow_rate real,
  speed_rpm integer,
  vibration double precision,
  CONSTRAINT pump_readings_point_key UNIQUE (site, unit_id, recorded_at)
);

COMMENT ON TABLE readings IS 'Boiler measurements, one row per point.';
COMMENT ON COLUMN readings.recorded_at IS 'Time of the measurement.';
COMMENT ON TABLE pump_readings IS 'Pump measurements, one row per point.';
COMMENT ON COLUMN pump_readings.recorded_at IS 'Time of the measurement.';
