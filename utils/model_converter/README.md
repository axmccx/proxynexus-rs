# Model Converter Utility

Regenerates `proxynexus-core/assets/realesr-general-x4v3.onnx`, the upscaling model used by Proxy Nexus,
from the upstream Real-ESRGAN PyTorch checkpoint, so that it can be used with Burn. 

## Reproducing the shipped asset

Requires `uv`.

```bash
uv run convert.py
```

With no arguments it downloads the checkpoint next to the script, checks its digest,
and writes the asset.

| File | SHA-256 |
|---|---|
| `realesr-general-x4v3.pth` (release [v0.2.5.0](https://github.com/xinntao/Real-ESRGAN/releases/tag/v0.2.5.0)) | `8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292` |
| `realesr-general-x4v3.onnx` | `d5a054f9e86dd186542cc20c3cebce97a089776c0d8f5e6cdd16ac8cac2d4d3e` |

These digests hold only for the versions pinned in the script header (`torch==2.11.0`,
`onnx==1.21.0`, `onnxscript==0.6.2`).

A checkpoint digest mismatch warns and continues, but the output will not match the table.

## Options

- `input` — checkpoint to convert. Default: `realesr-general-x4v3.pth` beside the script,
  downloaded from the upstream release if absent.
- `-o`, `--output` — where to write the `.onnx`. Default:
  `proxynexus-core/assets/realesr-general-x4v3.onnx`.

`num_conv`, `num_feat`, and the upscale factor are read from the checkpoint. The script exits if
the factor is not 4, which is what `SCALE` in `proxynexus-core/src/upscaler.rs` expects.

## Model

`realesr-general-x4v3` is an `SRVGGNetCompact` with `num_feat=64`, `num_conv=32`, `upscale=4`,
`act_type='prelu'` — 1,213,296 parameters.

The class in `convert.py` is transcribed from upstream
[`realesrgan/archs/srvgg_arch.py`](https://github.com/xinntao/Real-ESRGAN/blob/master/realesrgan/archs/srvgg_arch.py)
(BSD-3-Clause). Checkpoint keys are positional (`body.0`, `body.2`, …), so layer order must match
upstream; loading uses `strict=True`.

## Export

- Opset 18. Batch is fixed at 1; height and width are dynamic, so the upscaler can feed arbitrary
  tile sizes.
- Graph: 34 `Conv`, 33 `PRelu`, one `DepthToSpace`, one `Resize`, one `Add`.
- Written through an in-memory buffer. A path-based export can spill weights into a sidecar
  `.onnx.data`, which burn's importer does not read.
- Node metadata is stripped. It holds torch stack traces containing absolute paths, which change
  the bytes from machine to machine.

## Build pipeline

`proxynexus-core/build.rs` runs burn's `ModelGen` over the `.onnx` when the `upscaling` feature is
enabled, emitting Rust model code and a `.bpk` weights file into `OUT_DIR`. Native builds
`include_bytes!` the `.bpk`; `proxynexus-gui/build.rs` copies it to
`public/realesr-general-x4v3.bpk`, which the wasm build fetches at runtime.

The `.bpk` is gitignored and not byte-reproducible — an identical `.onnx` produces differing `.bpk`
digests between builds.
