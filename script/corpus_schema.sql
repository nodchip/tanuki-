PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS raw_source (
    id INTEGER PRIMARY KEY,
    site TEXT NOT NULL,
    event TEXT NOT NULL,
    year INTEGER NOT NULL,
    relative_path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    retrieved_at REAL NOT NULL,
    UNIQUE(site, relative_path, sha256)
);
CREATE TABLE IF NOT EXISTS position (
    id INTEGER PRIMARY KEY,
    position_key TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS logical_game (
    id INTEGER PRIMARY KEY,
    game_hash TEXT NOT NULL UNIQUE,
    initial_sfen TEXT NOT NULL,
    black_name TEXT NOT NULL,
    white_name TEXT NOT NULL,
    moves_json TEXT NOT NULL,
    primary_source_id INTEGER NOT NULL REFERENCES raw_source(id)
);
CREATE TABLE IF NOT EXISTS source_game (
    source_id INTEGER NOT NULL REFERENCES raw_source(id),
    game_id INTEGER NOT NULL REFERENCES logical_game(id),
    PRIMARY KEY(source_id, game_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS game_position (
    game_id INTEGER NOT NULL REFERENCES logical_game(id),
    ply INTEGER NOT NULL,
    position_id INTEGER NOT NULL REFERENCES position(id),
    move TEXT NOT NULL,
    PRIMARY KEY(game_id, ply)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS candidate (
    id INTEGER PRIMARY KEY,
    position_id INTEGER NOT NULL REFERENCES position(id),
    move TEXT NOT NULL,
    representative_game_id INTEGER REFERENCES logical_game(id),
    representative_ply INTEGER,
    source_id INTEGER REFERENCES raw_source(id),
    priority_key BLOB NOT NULL CHECK(typeof(priority_key) = 'blob' AND length(priority_key) = 88),
    active INTEGER NOT NULL DEFAULT 1,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    UNIQUE(position_id, move),
    CHECK((representative_game_id IS NULL) = (representative_ply IS NULL)),
    FOREIGN KEY(representative_game_id, representative_ply)
        REFERENCES game_position(game_id, ply)
);
CREATE TABLE IF NOT EXISTS candidate_adhoc_history (
    candidate_id INTEGER PRIMARY KEY REFERENCES candidate(id) ON DELETE CASCADE,
    position_sfen TEXT NOT NULL,
    history_json TEXT NOT NULL,
    source TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS candidate_adhoc_source (
    candidate_id INTEGER NOT NULL REFERENCES candidate(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    PRIMARY KEY(candidate_id, source)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS candidate_priority_idx
    ON candidate(active, priority_key DESC, id);
CREATE INDEX IF NOT EXISTS candidate_position_priority_idx
    ON candidate(position_id, active, priority_key DESC, id);
CREATE TABLE IF NOT EXISTS search_task (
    id INTEGER PRIMARY KEY,
    candidate_id INTEGER NOT NULL REFERENCES candidate(id),
    book_snapshot_id TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    lease_until REAL,
    last_error TEXT,
    eval_cp INTEGER,
    response TEXT,
    depth INTEGER,
    nodes INTEGER,
    engine_config_id TEXT,
    persisted_checkpoint_id INTEGER,
    updated_at REAL NOT NULL,
    UNIQUE(candidate_id, book_snapshot_id)
);
CREATE INDEX IF NOT EXISTS search_task_status_idx
    ON search_task(book_snapshot_id, status, lease_until);
CREATE TABLE IF NOT EXISTS ingest_error (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES raw_source(id),
    error TEXT NOT NULL,
    error_line INTEGER,
    previous_sfen TEXT,
    move TEXT,
    reason TEXT NOT NULL DEFAULT '',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS ranking_snapshot (
    id INTEGER PRIMARY KEY,
    site TEXT NOT NULL,
    event TEXT NOT NULL,
    source_url TEXT NOT NULL,
    retrieved_at REAL NOT NULL,
    source_sha256 TEXT NOT NULL,
    provisional INTEGER NOT NULL,
    policy_version TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS participant (
    id INTEGER PRIMARY KEY,
    event TEXT NOT NULL,
    official_name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    UNIQUE(event, official_name)
);
CREATE TABLE IF NOT EXISTS participant_alias (
    event TEXT NOT NULL,
    alias TEXT NOT NULL,
    participant_id INTEGER NOT NULL REFERENCES participant(id),
    mapping_status TEXT NOT NULL,
    PRIMARY KEY(event, alias)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS ranking_result (
    snapshot_id INTEGER NOT NULL REFERENCES ranking_snapshot(id),
    participant_id INTEGER NOT NULL REFERENCES participant(id),
    stage TEXT NOT NULL,
    stage_tier INTEGER NOT NULL,
    rank INTEGER NOT NULL,
    participants INTEGER NOT NULL,
    rank_percentile REAL NOT NULL,
    PRIMARY KEY(snapshot_id, participant_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS rating_snapshot (
    id INTEGER PRIMARY KEY,
    year INTEGER NOT NULL,
    snapshot_time REAL NOT NULL,
    source_url TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    anchor_era TEXT NOT NULL,
    rated_player_count INTEGER NOT NULL,
    component_size INTEGER NOT NULL,
    anchor_connected_rate REAL NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS rating_result (
    snapshot_id INTEGER NOT NULL REFERENCES rating_snapshot(id),
    player_name TEXT NOT NULL,
    rating REAL NOT NULL,
    effective_games REAL NOT NULL,
    connected_to_anchor INTEGER NOT NULL,
    reliability_bucket INTEGER NOT NULL,
    anchor_margin REAL NOT NULL,
    snapshot_percentile REAL NOT NULL,
    PRIMARY KEY(snapshot_id, player_name)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS progressive_width_history (
    id INTEGER PRIMARY KEY,
    old_width INTEGER NOT NULL,
    new_width INTEGER NOT NULL,
    changed_at REAL NOT NULL,
    reason TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS metric_counter (
    name TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS checkpoint (
    id INTEGER PRIMARY KEY,
    book_hash TEXT NOT NULL,
    created_at REAL NOT NULL
);
