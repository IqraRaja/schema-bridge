-- Made-up target database for the Schema Bridge demo: a shared catalogue
-- of assets, people and sessions. PostgreSQL. Contains no real data.
--
-- Recreate the demo exports (needs a scratch PostgreSQL database):
--   createdb catalogue && psql -d catalogue -f demo/sql/catalogue-postgresql.sql
--   psql -d catalogue -c "\copy ($(sed 's/--.*$//;s/;\s*$//' export-queries/postgresql.sql | tr '\n' ' ')) TO 'demo/catalogue-postgresql.csv' CSV HEADER"

CREATE TABLE asset_type(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name text NOT NULL UNIQUE,
  description text
);

CREATE TABLE location(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name text NOT NULL,
  building text
);

CREATE TABLE asset(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  asset_type_id uuid NOT NULL REFERENCES asset_type(uuid),
  parent_asset_id uuid,
  location_id uuid REFERENCES location(uuid),
  name text,
  code text NOT NULL UNIQUE,
  description text,
  add_info jsonb,
  start timestamp NOT NULL DEFAULT LOCALTIMESTAMP,  -- without time zone ...
  "end" timestamptz                                -- ... but this one with
);

-- Deliberate mistake: the parent link points to `location` instead of
-- `asset`, and was added without checking existing rows.
ALTER TABLE asset ADD CONSTRAINT asset_parent_fkey
  FOREIGN KEY (parent_asset_id) REFERENCES location(uuid) NOT VALID;

CREATE TABLE capability(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  asset_type_id uuid REFERENCES asset_type(uuid),
  name text NOT NULL,
  description text,
  available boolean NOT NULL
);

CREATE TABLE session(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name text NOT NULL,
  description text,
  time_start timestamptz NOT NULL,
  time_end timestamptz,
  add_info jsonb,
  CONSTRAINT session_time_check CHECK (time_end IS NULL OR time_end >= time_start)
);

CREATE TABLE person(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  first_name text NOT NULL,
  last_name text NOT NULL,
  orcid text                   -- identifier, but no unique rule
);

CREATE TABLE session_person(
  uuid uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  session_id uuid NOT NULL REFERENCES session(uuid),
  person_id uuid NOT NULL REFERENCES person(uuid),
  role text NOT NULL
);

COMMENT ON TABLE asset IS 'One physical asset (a single machine, sensor or instrument).';
COMMENT ON COLUMN asset.code IS 'Short unique code printed on the asset label.';
COMMENT ON TABLE capability IS 'What an asset type can do.';

ANALYZE;
