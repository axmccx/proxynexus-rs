# Proxy Nexus

Make high quality card game proxies.

Generate print-and-play PDFs or pre-formatted image files for MakePlayingCards.com by providing a list of card names, 
a set name, or a decklist URL.

The web app is hosted at https://proxynexus.net/, uses its own collection of high-quality card scans 
and official print-and-play images.

Proxy Nexus can also run locally as an offline desktop app or CLI. 
The CLI provides features to build and manage card image collections.
---

## Building & Running

### Prerequisites
- [Rust](https://rust-lang.org/learn/get-started/), 
- [dioxus-cli](https://dioxuslabs.com/learn/0.7/getting_started/) 
  provides the `dx` command for running the GUI
- `clang` (Linux only) — required to cross-compile C-based dependencies (e.g. `zstd-sys`)
  for the `wasm32-unknown-unknown` target. Install via `sudo apt install clang` on Debian/Ubuntu.


### Running the Web App Locally
```bash
dx serve --platform web
```
The web app fetches images from a Cloudflare R2 bucket, even when running locally, therefore it does not work offline. 
This is mostly for testing.


### Running the Desktop App Locally
```bash
dx serve
```
The desktop app runs locally, including its database and image file collections. You'll notice on first start that it won't
know of any card names or sets. **To make the Desktop app usable, you must first use the CLI to load a local card collection.**


### Building the CLI
```bash
cargo build -p proxynexus-cli --release
```
The built binary will be located at `./target/release/proxynexus-cli` (or `.\target\release\proxynexus-cli.exe` on Windows)

---

## Local Setup (CLI & Desktop)

When the CLI or desktop app runs for the first time, it synchronizes all card and set metadata for all supported games 
and saves it locally. The app then needs image files of cards, which are added from collection `.pnx` files. 
The CLI is able to create these collections from a folder of card scan image files, and manage them in the app.

### 1. Acquiring Images
To build a collection, you need a folder of correctly named card images. 
The file names in the folder **must** follow the [image file naming conventions](#image-file-naming-convention). 

#### Netrunner
You can find the images used to create the Netrunner collections here: 
[Google Drive - Proxy Nexus Collections](https://drive.google.com/drive/folders/1d84k6Od5bSBK31-lQkJzRc71xGx6-zVS?usp=sharing). 
This includes scans of FFG cards and images extracted from Null Signal Games (NSG) PDFs.

### 2. Building a Collection `.pnx` File
```bash
proxynexus-cli collection build --game netrunner --images ./core_set_scans --output core_set.pnx
```
Creates a collection file `core_set.pnx` for the game netrunner from all the images in the `core_set_scans` folder.


### 3. Adding the Collection
```bash
proxynexus-cli collection add core_set.pnx
```
Adds the new `core_set.pnx` collection. This updates the app's local database, and copies all the images to your
home drive under `~/.proxynexus/collections/`

The desktop app is now ready to use. You can also check which collections have been added using the command:

```bash
proxynexus-cli collection list
```

---

## CLI Commands

The `proxynexus-cli` supports the following subcommands. You can use `--help` on any command for more specific options.

**Generation:**
*   `generate pdf`: Generate a print-and-play PDF from a specific set, cardlist, or decklist URL.
*   `generate mpc`: Generate a MakePlayingCards (MPC) formatted ZIP file.

**Collection Management:**
*   `collection build`: Create a new `.pnx` collection file from a directory of card scans.
*   `collection add`: Load a `.pnx` collection into your local app.
*   `collection list`: View all loaded collections.
*   `collection remove`: Delete a collection from your local app.

**Catalog Management:**
*   `catalog update`: Fetch the latest catalog from each game's database API.
*   `catalog info`: View metadata about the local catalog.
*   `catalog import`: Import catalog data from local JSON files.

**Query & Export:**
*   `query`: Search the catalog and collections (e.g., `--list-sets` or `--set-name`).
*   `export`: Export the local database to an `init.sql` file. Required for the web app at `proxynexus-gui/public/init.sql`.

---

## Terminology

*   **Card:** Abstract representation of a card, uniquely identified by its title.
*   **Version:** An official retail release of a card.
*   **Printing:** A specific print of a card, directly associated to an image file. Can be an official or unofficial Version.
*   **Variant:** A label assigned to unofficial Printing. Can be an alt-art prize card or a custom card design. 
*   **Part:** Some printings have more than one image. Most cards just have a "front" part, but double-sided cards have a "back" part as well.
*   **Collection:** A set of card image files and metadata. Can be packaged into a `.pnx` file by the CLI, and added to a local Proxy Nexus instance.
*   **Pack and Set:** A retail expansion of cards. Both mean the same thing and are used interchangeably. 
*   **Card Request:** The user's intent when asking to generate a proxy. It specifies the card title and code and optional printing or collection overrides.

--- 

## Image File Naming Convention

Each image file represents a single printing and part. The collection builder relies solely on the file name to identify it.
The general syntax is:
`{card_id}@{printing}[~{part}].{extension}`

The Card ID and Printing sections are required. The part section is optional and defaults to "front" if omitted.
`printing` must be `pack_id` for official cards and can be any free-form label for unofficial art-art/custom 
Only PNG and JPEG files are supported.

#### File name scenarios:
*   **Standard Cards:** The majority of card image files. (e.g., `hedge_fund@core_set.jpg` -> ID: hedge_fund, Printing: core_set, Part: front).
*   **Alternate Art:** The printing must be an official pack or a custom label for alt-arts. (e.g., `hedge_fund@alt1.jpg` -> ID: hedge_fund, Printing: alt1, Part: front).
*   **Parts (Multiple Sides):** Contains a tilde `~` followed by the part name. 
(e.g., `sync_everything_everywhere@data_and_destiny~back` -> ID: sync_everything_everywhere, Printing: data_and_destiny, Part: back).

**Strict Rules:**
*   **Orphans:** If a part file doesn't have an associated front file, it is ignored.
*   **Exact API IDs:** For official printings, the `{card_id}` and `{printing}` (pack ID) **must** exactly match the 
string IDs used by the game's respective database API.

---

## Card Requests and Printing Notation

#### Printing Notation in Card Lists

When generating from a card list, you can request a specific printing (an official pack or custom variant label), or collection using the following notation.
`Quantity CardName [printing:collection]`

Examples:
*   **Requesting a specific printing:** `3x Sure Gamble [alt1]`
*   **Requesting a specific collection:** `3x Sure Gamble [:ffg-en]`
*   **Requesting a specific printing from a specific collection:** `3x Snare! [alt1:extras]`
*   **Requesting a specific official pack:** `3x Hedge Fund [revised_core_set]`

The printing notation is optional.

**Discovering Available Printings:**

You can use the CLI's `query` command to see what's available.

The following lists the number of Printings per set, per collection:
```bash
./proxynexus-cli query --list-sets

Available Sets:

  - Core Set                    [core_set]                       # 75 in ffg-en
  - Draft                       [draft]                          # 9 in ffg-en
  - What Lies Ahead             [what_lies_ahead]                # 18 in ffg-en
  - Trace Amount                [trace_amount]                   # 20 in ffg-en
...
```

The following lists the printings and the collection they're in for each card in the set:
```bash
./proxynexus-cli query --set-name "Core Set"

Query Results:

1x Noise: Hacker Extraordinaire [core:ffg-en]  # also: [alt1:ffg-en], [alt1:extras]
2x Déjà Vu [core:ffg-en]                       # also: [alt1:ffg-en]
3x Demolition Run [core:ffg-en]
3x Stimhack [core:ffg-en]                      # also: [alt1:ffg-en]
...
```
The quantity comes from the pack's metadata from the game's API. The output of this query is a valid card list.

#### Card Request Resolution

Whether you're using the notation above in a card list, or selecting a set name, or a decklist URL, 
the app converts this input into a list of **Card Requests**. 

Each Card Request in the list is then used to find the best available Printing, across all available collections,
using the following priority hierarchy:
1.  Match the requested printing. If no printing is specified, prefer official printings over custom variants.
2.  Match the exact collection, if provided.
3.  Use the oldest chronological printing available.

---

## Updating the Web App's Collections

These steps aren't useful without access to the Cloudflare R2 bucket, but I'm including them here for posterity.

The web app is almost the same as the desktop app, but it doesn't include the collection management features,
making its database effectively read-only.


1.  Use the CLI to remove the old version of the collection (if replacing an existing one), and then add the new collection.
    ```bash
    proxynexus-cli collection remove <collection_name>
    proxynexus-cli collection add <new_collection.pnx>
    ```

2.  Sync the local `~/.proxynexus/collections` directory up to that bucket.
    ```bash
    rclone sync ~/.proxynexus/collections r2-bucket-name:proxynexus-collections --progress
    ```

3.  Export the local DB, containing the new collection metadata, as a new `init.sql` payload that the web app hydrates from.
    ```bash
    proxynexus-cli export --output proxynexus-gui/public/init.sql
    ```

4.  Run the web app locally (`dx serve --platform web`) to ensure the new `init.sql` loads correctly
    and the images are fetching from R2 as expected.

5.  Commit the updated `init.sql` file and merge it to `master`. GitHub will build and deploy the web app release files to Cloudflare Pages.

---

## Technical Notes

### Image Pre-Processing

#### Corner Infill
You might notice that real FFG cards have rounded corners, but the images used by Proxy Nexus are rectangular. 
This is because all images have been processed with the **Corner Infill Script**, located in `utils/corner_infill/`. 
This script uses OpenCV to detect the blank white corners of raw card scans, and fills them in using the Navier-Stokes
inpainting algorithm (`cv2.inpaint`). 

#### Page Slicer
To extract the raw image files from the NSG Print and Play PDFs, I use [pdfimager](https://github.com/sckott/pdfimager).
For some PDFs, each card is saved as a separate image. For others, only full-page 3x3 grid images are saved.
In order to "slice" these full page images, I used the **NSG Page Slicer Script**, located in  `utils/nsg_page_slicer/`. 
For more details on how this script works, please refer to the `utils/nsg_page_slicer/README.md`.

### MPC Processing

When generating images formatted for PDFs, images are used as-is from their collections.
However, when generating for MakePlayingCards.com, additional processing is done to each image on-the-fly.

#### Image Scaling & Edge Replication
When printing physical proxies through MakePlayingCards (MPC), images require a print-safe bleed border to meet their 
recommended minimum resolution of 816x1110 pixels (for a 744x1038 cut size). 
The old Proxy Nexus website used an entirely duplicate set of images, which were pre-processed with this bleed border.
That pre-processing relied on OpenCV, just like the corner infilling does. However, with this project's goal of 
supporting flexible collection management, and being written in Rust targeting WASM for the web app, 
OpenCV could not be used for its copyMakeBorder function. 

Instead, the `proxynexus-core` contains its own `add_bleed_border` function in `proxynexus-core/src/print_prep.rs` which 
processes each image dynamically:
*   **Image Scaling:** If the original image is smaller than the MPC cut line (which is often the case for 
NSG print-and-play extracts), it uses the Lanczos3 algorithm to scale the image just enough so the longest side reaches 
the cut line. This prevents original art from being cropped by MPC while preserving the aspect ratio and image quality. 
Images that are already large enough remain unchanged.
*   **Dynamic Bleed Generation:** Rather than adding a strict 36px bleed all around, the bleed is dynamically sized. 
It iteratively copies the outer edge pixels and rapidly blits them outward to create a seamless bleed natively in Rust. 
This ensures at least a 36px bleed while padding the shorter sides to guarantee the final image hits the minimum 
MPC size of 816x1110.

I benchmarked this function against a version that used the Rust bindings of OpenCV's copyMakeBorder, and while mine is 
slower, it's quite good enough for keeping the project as purely Rust as possible.

#### The Uniqueness Marker
Most orders on MPC will contain duplicates of the same image. It's also very convenient to use their
"place images for me" autofill feature. However, MPC's image upload will notice when duplicates of the same identical
image are uploaded, and skip them. This effectively breaks the autofill feature, meaning you'd need to manually
place the same image for the number of copies you want.

To bypass this, the MPC generation process applies a "Uniqueness Marker" (`apply_uniqueness_marker` in `print_prep.rs`).
It imperceptibly alters the RGB values of the top-left 2x2 pixels using a pseudo-random addition based on the number
of copies being made. These altered pixels get cut off anyways, because they're well in the bleed border.
This guarantees every file inside the generated `.zip` is technically unique as far as MPC is
concerned, and every file gets uploaded.

### Image Caching

When generating a large list of cards, it's likely that the app will be fetching and using the same image file more than once.
To save on network bandwidth and processing time, both the PDF and MPC generation processes make use of caching.

*   **PDF Generation:** Once the image bytes from the provider are obtained, it is only parsed into a `krilla::Image` structure once,
and stored in the cache. This cached copy is then used when adding additional copies of this same image to the PDF.
*   **MPC Generation:** This follows a similar process except it caches the image *after* the heavy bleed border is applied, 
but *before* the uniqueness marker is stamped. This ensures the expensive `add_bleed_border` function only runs once per file, 
while still allowing the fast uniqueness marker to stamp each individual copy just before it is written to the zip archive. 

### File Generation Logic 

Here's the high-level flow for generating an output file:

1.  **Connect to the Database.** When running locally, the app connects to the local Sled DB, while the web app 
sets up an in-memory DB and hydrates it using the remote `init.sql` file. This DB connection is passed to the `CardStore`, 
which handles querying the DB, entirely unaware of its underlying storage.

2.  **Determine the CardSource.** Based on the user's source selection, their input is wrapped in a specific struct 
(`Cardlist`, `SetName`, or `DecklistUrl`), all of which implement the `CardSource` trait.

3.  **Generate CardRequests.** The `CardStore` instance is passed to `to_card_requests()`, a method defined 
by the `CardSource` trait, to produce a list of CardRequests. Whether it's querying the DB for a card list, 
all cards in a set, or making an async request to a decklist API, the caller is unaware of both the 
underlying DB storage and querying logic used by each source.

4.  **Resolve Printings.** The `CardStore`'s `resolve_printings` method queries the database to find all 
available Printings that match the requests, applying the fallback logic and variant overrides discussed in 
the "Card Request Resolution" section.

5.  **Fetch Images.** An `ImageProvider` is initialized. When running locally, the app uses a `LocalImageProvider` to 
read from local storage, while the Web App uses a `RemoteImageProvider` to fetch asynchronously from Cloudflare R2.

6.  **Generate Output.** The resolved list of Printings and the `ImageProvider` are passed to 
either `generate_pdf` or `generate_mpc_zip`. These core builders process the images and write the final file. 
They work exactly the same whether the images came from a local hard drive or a remote server.

This core process took a lot of trial and error to nail down. In my day-to-day work in Python, I don't tend to reach
for object-oriented designs right away, but I realized very quickly I needed to do something to handle code reuse
and separation of concerns. Rust's traits took some time to wrap my head around, but once it clicked, it felt good.

---

## Database Architecture

**The Starting Point: `rusqlite`**
When starting out with the CLI, I used `rusqlite` with a local `~/.proxynexus/proxynexus.db` file.
But once I started POC'ing the web app in WASM, I found out that `rusqlite` relies on C-bindings,
which would be a challenge to compile and setup in WASM.

**Detour 1: The `turso` Migration**
Doing some research, and unfortunately listening to poor advice from an LLM, I migrated to `turso`,
claiming to be written in pure Rust and having support for WASM. It looked promising, so I refactored all the DB
querying to async Rust (which in itself wasn't a terrible idea), so that I could use `turso`.
However, when attempting to initialize a `:memory:` database in the browser, `turso` crashed with a
panic: `time not implemented on this platform`. It was then I learned that it targets serverless WASM
(like WASI environments), not standard browsers. It hardcodes calls to OS-level standard library
features (like `std::time::SystemTime`) that standard browsers do not provide. Super frustrating to find that out.

**Detour 2 & 3: Dioxus Fullstack and `libsql`**
For a moment, I considered Dioxus' fullstack features, but gave that up because I really didn't want to deal with
hosting a backend database. I then considered Turso's managed cloud database service, because at least they had a free tier.
I was hoping the WASM client could at least send SQL strings to the Turso cloud API. So I swapped the `turso` crate
for the official `libsql` crate to try its `new_remote` builder. Unfortunately, it also failed to compile,
it's just not meant to run in a browser.

**The Final Pivot: `gluesql`**
Researching another option, I discovered `gluesql`. It's a SQL database engine written entirely in safe Rust,
so it had to be compatible with the `wasm32-unknown-unknown` target. It also advertised supporting
"a variety of storage options". So I used:
*   `SledStorage` for the local CLI and desktop app, to persistently save data to the local hard drive.
*   `MemoryStorage` for the web app, to use the DB in memory. Because it starts empty on each page load, 
the web app fetches the `init.sql`, an export of my native DB instance, and hydrates the `MemoryStorage` on startup.

Switching to `gluesql` required another small refactor, because it doesn't support features like indexes
and foreign keys. These weren't a huge loss though, because the DB schema is quite simple anyways. I'm super happy
with how this solution turned out.

---

## Rebuilt in Rust

This repo is a rebuild of [the old Proxy Nexus](https://github.com/axmccx/proxynexus/), and it aims to improve all the flaws of that version.

Having a backend server generate each request was a huge flaw. The server was cheap to run but not free.
At the time, I felt clever building its caching system, but it would frequently run out of storage space, requiring
automated scripts to clear the cache and reboot the server. It sucked to see people online saying that the
website was down.

The database design wasn't great either. It relied on seed files to populate the DB, making it
super tedious to update the website with new cards. Admittedly, I never put much thought into the process of ongoing
card updates. I figured eventually everyone would just use NSG's print-and-play PDFs. I didn't want to simply fetch
images from netrunnerdb, because I took pride in offering the highest quality proxies. 

I built the old website with Node.js on the backend and vanilla JS on the frontend. It felt good to avoid
using a frontend framework and keeping things lightweight, but man, the backend became so ugly and unpleasant to work with.
Lastly, I felt the use of Azure blob storage for the images and caching made the architecture opaque and difficult 
for anyone else to stand up the website on their end and make contributions.

Since last year, I've been learning Rust in my free time and decided that building a Proxy Nexus CLI in Rust 
would be a good learning exercise.
Since Rust is fun to use, I went looking for a UI framework and found Dioxus. Fascinated with its support for WASM web apps
as a compile target, it filled me with motivation to rebuild the website in Rust, just to see how well that could work.

In addition to supporting all the existing features, I had the following goals:

* Be able to run everything locally. All image processing and the database should be able to run entirely in the browser.
* Be free to host. Though the web version still needs a hosting service, luckily Cloudflare R2's free tier is good enough.
* Enable anyone to manage card images. This was a foundational change, but it would push me to set up the project 
in such a way that adding new cards would be as easy as possible. While I plan to keep the hosted web app updated, 
I would feel incredibly accomplished to see someone else create and share their own collection `.pnx` file!

This rebuilt succeeds at making the website faster, more stable, free to host, a pleasure for me to work on,
and hopefully easier for anyone to dive into the codebase.
