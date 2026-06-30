-- 001_init.sql
-- Forward-only migration (OPS-0008 / BE-0007). One DDL set per file; fix forward,
-- never edit an applied migration in place. Vitess: no FKs — enforce integrity in the access seam.
-- Applied by `midas migrate` (and automatically by `midas dev` once the tunnel is up).

CREATE TABLE items (
    id         VARCHAR(32) NOT NULL,
    user_id    VARCHAR(32) NOT NULL,
    created_at BIGINT      NOT NULL,
    PRIMARY KEY (id),
    KEY idx_items_user_id (user_id)
);
