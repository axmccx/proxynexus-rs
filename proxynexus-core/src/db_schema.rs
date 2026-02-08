use rusqlite::Connection;

pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS cards (
            code TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            set_code TEXT NOT NULL,
            set_name TEXT NOT NULL,
            release_date TEXT,
            side TEXT NOT NULL,
            quantity INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS printings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            card_code TEXT NOT NULL,
            variant_type TEXT NOT NULL,
            variant_number INTEGER,
            image_path TEXT NOT NULL,
            FOREIGN KEY (card_code) REFERENCES cards(code)
        );

        CREATE INDEX IF NOT EXISTS idx_printings_card ON printings(card_code);
        CREATE INDEX IF NOT EXISTS idx_cards_set ON cards(set_code);
        ",
    )?;

    Ok(())
}

pub fn insert_card(
    conn: &Connection,
    card: &crate::collection::CardMetadata,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO cards
         (code, title, set_code, set_name, release_date, side, quantity)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            &card.code,
            &card.title,
            &card.set_code,
            &card.set_name,
            &card.release_date,
            &card.side,
            &card.quantity,
        ],
    )?;

    Ok(())
}

pub fn insert_printing(
    conn: &Connection,
    printing: &crate::collection::Printing,
) -> rusqlite::Result<()> {
    let (variant_type, variant_number) = match &printing.variant {
        crate::collection::PrintingVariant::Default => ("default", None),
        crate::collection::PrintingVariant::Alt(n) => ("alt", Some(*n as i64)),
        crate::collection::PrintingVariant::Rear => ("rear", None),
    };

    conn.execute(
        "INSERT INTO printings (card_code, variant_type, variant_number, image_path)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            &printing.card_code,
            variant_type,
            variant_number,
            &printing.image_path,
        ],
    )?;

    Ok(())
}
