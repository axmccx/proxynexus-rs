import csv
import requests

CARDS_API_URL = "https://netrunnerdb.com/api/2.0/public/cards"
PACKS_API_URL = "https://netrunnerdb.com/api/2.0/public/packs"
OUTPUT_FILE = "cards.csv"


def fetch_json(url):
    response = requests.get(url, timeout=30)
    response.raise_for_status()
    return response.json()["data"]


def build_pack_lookup(packs):
    return {
        pack["code"]: {
            "name": pack.get("name", ""),
            "release_date": pack.get("date_release", ""),
        }
        for pack in packs
    }


def write_csv(cards, pack_lookup, filename):
    fieldnames = [
        "code",
        "title",
        "set_code",
        "set_name",
        "release_date",
        "side",
        "quantity",
    ]

    with open(filename, "w", newline="", encoding="utf-8") as csvfile:
        writer = csv.DictWriter(csvfile, fieldnames=fieldnames)
        writer.writeheader()

        for card in cards:
            pack_code = card.get("pack_code", "")
            pack_info = pack_lookup.get(pack_code, {})

            row = {
                "code": card.get("code", ""),
                "title": card.get("title", ""),
                "set_code": pack_code,
                "set_name": pack_info.get("name", ""),
                "side": card.get("side_code", ""),
                "quantity": card.get("quantity", ""),
                "release_date": pack_info.get("release_date", ""),
            }
            writer.writerow(row)


def main():
    cards = fetch_json(CARDS_API_URL)
    packs = fetch_json(PACKS_API_URL)
    pack_lookup = build_pack_lookup(packs)
    write_csv(cards, pack_lookup, OUTPUT_FILE)
    print(f"Wrote {len(cards)} cards to {OUTPUT_FILE}")


if __name__ == "__main__":
    main()
