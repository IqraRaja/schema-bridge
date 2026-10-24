-- A small made-up "library" database that triggers the problem checks the
-- sample databases do not. Used only by tests/checks_parity.rs.
-- Rebuild the export: create a database, run this file, then run
-- export-queries/postgresql.sql and save the result as library-postgresql.csv.

CREATE FUNCTION uuid_generate_v4() RETURNS uuid LANGUAGE sql AS 'SELECT gen_random_uuid()';

CREATE TABLE branch (
  id integer CONSTRAINT fk_branch PRIMARY KEY,     -- primary key named like a link
  name text NOT NULL
);

CREATE TABLE member (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  email text UNIQUE,                              -- unique, but may be empty
  password text,                                  -- sensitive
  api_token text,                                 -- sensitive
  favourite_ids uuid[],                           -- list of IDs
  branch_code smallint,                           -- shared code (type 1)
  "user" text,                                    -- reserved word
  joined_at timestamptz,
  left_at timestamp                               -- mixed time zones
);
COMMENT ON COLUMN member.email IS 'Contact address.';

CREATE TABLE book (
  id uuid PRIMARY KEY DEFAULT uuid_generate_v4(), -- second ID generator
  title text NOT NULL,
  isbn text,                                      -- identifier without unique rule
  summary_xml text,
  summary_json jsonb                              -- same data twice
);
CREATE INDEX ix_book_title ON book (title);
CREATE INDEX ix_book_title_again ON book (title); -- duplicate index

CREATE TABLE book_copy (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  shelf text
);

CREATE TABLE loan (
  id integer PRIMARY KEY,
  member_id uuid,
  book_id uuid CONSTRAINT pk_loan_book REFERENCES book_copy (id), -- named like a key; name suggests "book"
  external_ref uuid,                              -- UUID with no link
  branch_code integer,                            -- shared code (type 2)
  "order" integer                                 -- reserved word
);
ALTER TABLE loan ADD CONSTRAINT "Loan Member" FOREIGN KEY (member_id) REFERENCES member (id) NOT VALID;
CREATE INDEX ix_loan_member ON loan (member_id);

CREATE TABLE loan_item (
  loan_id integer REFERENCES loan (id),
  book_id uuid REFERENCES book (id),
  id integer,                                     -- extra id, not unique
  PRIMARY KEY (loan_id, book_id)
);

CREATE TABLE audit_log (                          -- no primary key
  happened_at timestamptz,
  note text
);
INSERT INTO branch VALUES (1, 'Main');
ANALYZE branch;
