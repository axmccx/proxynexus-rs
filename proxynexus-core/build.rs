use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    generate_card_backs();

    #[cfg(feature = "upscaling")]
    {
        use burn_onnx::ModelGen;
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let onnx_file = "assets/realesr-general-x4v3.onnx";
        println!("cargo:rerun-if-changed={}", onnx_file);

        ModelGen::new()
            .input(onnx_file)
            .out_dir(&out_dir)
            .run_from_script();

        let bpk_path = std::path::Path::new(&out_dir).join("realesr-general-x4v3.bpk");
        if bpk_path.exists() {
            println!("cargo:metadata_model_weights_path={}", bpk_path.display());
        }
    }
}

/// Turns each game adapter's `backs/` folder into `card_back_table.rs`, the static table `card_backs.rs` reads.
fn generate_card_backs() {
    let games_dir = Path::new("src/games");
    println!("cargo:rerun-if-changed={}", games_dir.display());

    let mut groups = String::new();
    let mut bytes = String::new();

    let mut games: Vec<_> = fs::read_dir(games_dir)
        .expect("src/games is missing")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("backs").is_dir())
        .collect();
    games.sort();

    for game_dir in games {
        let backs_dir = game_dir.join("backs");
        println!("cargo:rerun-if-changed={}", backs_dir.display());

        // A folder is a Rust module and cannot contain a hyphen, but a game id can
        let module = game_dir.file_name().unwrap().to_str().unwrap().to_string();
        let game_id = module.replace('_', "-");

        let mut entries = String::new();
        let mut files: Vec<_> = fs::read_dir(&backs_dir)
            .expect("unreadable backs folder")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();

        for path in files {
            let file = path.file_name().unwrap().to_str().unwrap().to_string();
            let stem = path.file_stem().unwrap().to_str().unwrap();

            let (stem, has_bleed) = match stem.strip_suffix(".bleed") {
                Some(stripped) => (stripped, true),
                None => (stem, false),
            };

            let Some((back_group, label)) = stem.split_once('_') else {
                panic!(
                    "{}: a card back is named {{back_group}}_{{label}}.{{ext}}, and '{}' has no '_'",
                    backs_dir.display(),
                    file
                );
            };
            assert!(
                !back_group.is_empty() && !label.is_empty(),
                "{}: '{}' has an empty back group or label",
                backs_dir.display(),
                file
            );

            let asset_id = format!("{}_{}", game_id, file);
            let absolute = fs::canonicalize(&path).unwrap();

            writeln!(
                entries,
                "        CardBack {{ back_group: \"{back_group}\", label: \"{label}\", file: \"{file}\", asset_id: \"{asset_id}\", has_bleed: {has_bleed} }},"
            )
            .unwrap();
            writeln!(
                bytes,
                "    (\"{}\", include_bytes!(r\"{}\")),",
                asset_id,
                absolute.display()
            )
            .unwrap();
        }

        writeln!(groups, "    (\"{game_id}\", &[\n{entries}    ]),").unwrap();
    }

    let generated = format!(
        "pub static CARD_BACKS: &[(&str, &[CardBack])] = &[\n{groups}];\n\n\
         #[cfg(not(target_arch = \"wasm32\"))]\n\
         pub static CARD_BACK_BYTES: &[(&str, &[u8])] = &[\n{bytes}];\n"
    );

    let out_dir = std::env::var("OUT_DIR").unwrap();
    fs::write(Path::new(&out_dir).join("card_back_table.rs"), generated)
        .expect("failed to write the generated card back table");
}
