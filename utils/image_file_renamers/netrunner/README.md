# Netrunner Image File Renamer

Renames Netrunner card scans from the legacy code-based filenames to the current
[image file naming convention](../../../README.md#image-file-naming-convention).

```
legacy    01001_alt1.jpg
current   hedge_fund@alt1.jpg
```

> **Renames files IN PLACE (`os.rename`).** No output folder, no undo. Run it against a disposable
> copy of your scans.

## How it maps

Legacy names are `{code}_{variant}[-{part}].{ext}`, where `code` is NetrunnerDB's 5-digit printing
code. That code is the `id` of a printing in the NRDB v3
[printings catalog](https://api.netrunnerdb.com/api/v3/public/printings), which carries the
slugs the current convention needs:

- **card_id** — the printing's `card_id`.
- **printing** — the filename's variant label if it has one (`01001_alt1` → `alt1`), otherwise the
  printing's `card_set_id`.
- **part** — a printing is one front and a sequence of backs numbered from one, with index one
  spelled without its number. `-front`/`-front1` become no suffix, `-back1` becomes `~back`, and
  `-face2`/`-face3`/`-face4` become `~back`/`~back2`/`~back3`. A repeated front (`-front2` and up)
  is a copy of the one front, so those files are printed and left alone rather than renamed.

The catalog is cached as `printings_cache.json` inside the folder being processed, downloaded from
the API on first run.

## Running it

```bash
uv run rename.py /path/to/copy-of-scans/ --dry-run   # preview
uv run rename.py /path/to/copy-of-scans/             # apply
```

## Known limitations

- **Destructive, no undo.**
- **5-digit codes only.** Filenames that don't fit are ignored silently — not even logged as
  skipped — so a folder of unrecognised names looks like a successful no-op.
