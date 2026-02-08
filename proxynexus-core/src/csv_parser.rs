use crate::collection::CardMetadata;
use csv::ReaderBuilder;
use std::path::Path;

#[derive(Debug)]
pub enum CsvError {
    Csv(csv::Error),
    MissingField { row: usize, field: &'static str },
    InvalidQuantity { row: usize, value: String },
}

impl From<csv::Error> for CsvError {
    fn from(err: csv::Error) -> Self {
        CsvError::Csv(err)
    }
}

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsvError::Csv(e) => write!(f, "CSV error: {}", e),
            CsvError::MissingField { row, field } => {
                write!(f, "Missing required field '{}' at row {}", field, row)
            }
            CsvError::InvalidQuantity { row, value } => {
                write!(f, "Invalid quantity '{}' at row {}", value, row)
            }
        }
    }
}

impl std::error::Error for CsvError {}

pub fn parse_csv(path: &Path) -> Result<Vec<CardMetadata>, CsvError> {
    let mut reader = ReaderBuilder::new().has_headers(true).from_path(path)?;

    let headers = reader.headers()?.clone();
    let mut cards = Vec::new();

    for (idx, result) in reader.records().enumerate() {
        let record = result?;
        let row_num = idx + 2; // +2 because 0-indexed and skip header row

        let get_field = |name: &'static str| -> Result<String, CsvError> {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|pos| record.get(pos))
                .map(|s| s.to_string())
                .ok_or(CsvError::MissingField {
                    row: row_num,
                    field: name,
                })
        };

        let get_optional = |name: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|pos| record.get(pos))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        };

        let code = get_field("code")?;
        let title = get_field("title")?;
        let set_code = get_field("set_code")?;
        let set_name = get_field("set_name")?;
        let side = get_field("side")?;

        let quantity_str = get_field("quantity")?;
        let quantity = quantity_str
            .parse::<u32>()
            .map_err(|_| CsvError::InvalidQuantity {
                row: row_num,
                value: quantity_str.clone(),
            })?;

        let release_date = get_optional("release_date");

        cards.push(CardMetadata {
            code,
            title,
            set_code,
            set_name,
            release_date,
            side,
            quantity,
        });
    }

    Ok(cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_csv_valid() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            "code,title,set_code,set_name,release_date,side,quantity"
        )
        .unwrap();
        writeln!(
            file,
            "01001,Noise: Hacker Extraordinaire,core,Core Set,2012-09-06,runner,1"
        )
        .unwrap();
        writeln!(file, "01002,Déjà Vu,core,Core Set,2012-09-06,runner,2").unwrap();

        let cards = parse_csv(file.path()).unwrap();
        assert_eq!(cards.len(), 2);

        assert_eq!(cards[0].code, "01001");
        assert_eq!(cards[0].title, "Noise: Hacker Extraordinaire");
        assert_eq!(cards[0].set_code, "core");
        assert_eq!(cards[0].set_name, "Core Set");
        assert_eq!(cards[0].release_date, Some("2012-09-06".to_string()));
        assert_eq!(cards[0].side, "runner");
        assert_eq!(cards[0].quantity, 1);

        assert_eq!(cards[1].code, "01002");
        assert_eq!(cards[1].title, "Déjà Vu");
        assert_eq!(cards[1].quantity, 2);
    }
}
