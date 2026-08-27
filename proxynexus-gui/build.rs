use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest_dir).parent().unwrap();

    publish_card_backs(root, Path::new(&manifest_dir));

    if let Ok(bpk_path_str) = std::env::var("DEP_PROXYNEXUS_CORE_METADATA_MODEL_WEIGHTS_PATH") {
        let bpk_path = Path::new(&bpk_path_str);
        if bpk_path.exists() {
            let dest_path = Path::new(&manifest_dir)
                .join("public")
                .join("realesr-general-x4v3.bpk");
            fs::copy(bpk_path, &dest_path).expect("Failed to pull model weights from core");

            println!("cargo:rerun-if-changed={}", bpk_path_str);
        }
    }
}

fn publish_card_backs(root: &Path, manifest_dir: &Path) {
    let games_dir = root.join("proxynexus-core").join("src").join("games");
    let dest_dir = manifest_dir.join("public").join("card_backs");

    println!("cargo:rerun-if-changed={}", games_dir.display());

    if dest_dir.exists() {
        fs::remove_dir_all(&dest_dir).expect("Failed to clear the card_backs directory");
    }
    fs::create_dir_all(&dest_dir).expect("Failed to create the card_backs directory");

    let Ok(games) = fs::read_dir(&games_dir) else {
        return;
    };

    for game in games.flatten() {
        let backs_dir = game.path().join("backs");
        if !backs_dir.is_dir() {
            continue;
        }

        let game_id = game.file_name().to_string_lossy().replace('_', "-");

        for back in fs::read_dir(&backs_dir)
            .expect("Unreadable backs folder")
            .flatten()
        {
            let path = back.path();
            if !path.is_file() {
                continue;
            }

            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let dest_path = dest_dir.join(format!("{}_{}", game_id, file_name));

            fs::copy(&path, &dest_path).unwrap_or_else(|e| {
                panic!(
                    "Failed to copy {} to {}: {}",
                    path.display(),
                    dest_path.display(),
                    e
                )
            });
        }
    }
}
