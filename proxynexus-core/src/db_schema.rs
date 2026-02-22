use rusqlite::Connection;

pub fn create_app_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS collections (
            id INTEGER PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            version TEXT,
            language TEXT,
            added_date TEXT NOT NULL,
            last_updated TEXT
        );

        CREATE TABLE IF NOT EXISTS cards (
            code TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            title_normalized TEXT NOT NULL,
            set_code TEXT NOT NULL,
            set_name TEXT NOT NULL,
            release_date TEXT,
            side TEXT NOT NULL,
            quantity INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS printings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            collection_id INTEGER NOT NULL,
            card_code TEXT NOT NULL,
            variant TEXT NOT NULL,
            file_path TEXT NOT NULL,
            UNIQUE(collection_id, card_code, variant),
            FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
            FOREIGN KEY (card_code) REFERENCES cards(code) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_cards_code ON cards(code);
        CREATE INDEX IF NOT EXISTS idx_cards_title_normalized ON cards(title_normalized);
        CREATE INDEX IF NOT EXISTS idx_printings_card_code ON printings(card_code);
        CREATE INDEX IF NOT EXISTS idx_printings_collection ON printings(collection_id);
        ",
    )?;

    Ok(())
}
