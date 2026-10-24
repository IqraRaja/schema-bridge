-- Made-up source database for the Schema Bridge demo: a small workshop
-- that runs machines. SQLite. Contains no real data.
--
-- Recreate the demo export:
--   sqlite3 workshop.db < demo/sql/workshop-sqlite.sql
--   sqlite3 -header -csv workshop.db < export-queries/sqlite.sql > demo/workshop-sqlite.csv

CREATE TABLE machine(
  id INTEGER PRIMARY KEY,
  label VARCHAR(80) NOT NULL,
  serial_no TEXT,              -- identifier, but no unique rule
  room TEXT,
  power_kw REAL,
  notes TEXT
);

CREATE TABLE machine_function(
  id INTEGER PRIMARY KEY,
  machine_id INTEGER,          -- looks like a link, but no foreign key
  code SMALLINT,
  title VARCHAR(60),
  enabled BOOLEAN
);

CREATE TABLE operator(
  id INTEGER PRIMARY KEY,
  full_name VARCHAR(100) NOT NULL,
  login VARCHAR(30),
  password_hash TEXT
);

CREATE TABLE run(
  id INTEGER PRIMARY KEY,
  title VARCHAR(100),
  machine_id INTEGER REFERENCES machine(id),
  operator_id INTEGER REFERENCES operator(id),
  started DATETIME,
  ended DATETIME,
  station_nr SMALLINT DEFAULT 0,  -- plain number, no list of valid stations
  settings_xml TEXT,              -- same settings stored twice
  settings_json TEXT
);

CREATE TABLE booking(
  id INTEGER PRIMARY KEY,
  machine_id INTEGER REFERENCES machine(id),
  station_nr SMALLINT,
  start_at DATETIME,
  end_at DATETIME
);

CREATE INDEX ix_run_machine ON run(machine_id);
