pub const DDL: &str = "
CREATE TABLE IF NOT EXISTS l5r_cards (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_extra TEXT,
    side TEXT NOT NULL,
    card_type TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS l5r_card_versions (
    card_id TEXT NOT NULL,
    pack_id TEXT NOT NULL,
    image_url TEXT,
    quantity INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS l5r_packs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    released_at TEXT,
    cycle_id TEXT NOT NULL
);
";
