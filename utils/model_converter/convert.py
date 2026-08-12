# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "torch==2.11.0",
#     "onnx==1.21.0",
#     "onnxscript==0.6.2",
# ]
# ///
"""Convert the Real-ESRGAN `realesr-general-x4v3` checkpoint to the ONNX file used by the
Proxy Nexus upscaler (`proxynexus-core/assets/realesr-general-x4v3.onnx`).

The `SRVGGNetCompact` definition in this script is transcribed from upstream Real-ESRGAN
(`realesrgan/archs/srvgg_arch.py`, BSD-3-Clause) so the checkpoint loads with `strict=True`
and the exported graph matches what the released weights were trained against:
https://github.com/xinntao/Real-ESRGAN/blob/master/realesrgan/archs/srvgg_arch.py

Run with no arguments to fetch the weights and overwrite the checked-in asset; the export is
byte-reproducible, so a successful run prints the same SHA-256 recorded in README.md.
"""

import argparse
import hashlib
import io
import sys
import urllib.request
from pathlib import Path

import onnx
import torch
import torch.nn.functional as F
from torch import nn

# Upstream release asset for the model we ship. Both values are checked at runtime: the URL is
# only used when no local `.pth` is given, and the digest guards against a corrupt or swapped
# checkpoint silently producing a different model.
WEIGHTS_URL = "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesr-general-x4v3.pth"
WEIGHTS_SHA256 = "8dc7edb9ac80ccdc30c3a5dca6616509367f05fbc184ad95b731f05bece96292"

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent
DEFAULT_WEIGHTS = SCRIPT_DIR / "realesr-general-x4v3.pth"
DEFAULT_OUTPUT = REPO_ROOT / "proxynexus-core" / "assets" / "realesr-general-x4v3.onnx"

# Must match `SCALE` in proxynexus-core/src/upscaler.rs, which assumes a 4x model.
EXPECTED_SCALE = 4
OPSET = 18


class SRVGGNetCompact(nn.Module):
    """Compact VGG-style super-resolution network used by `realesr-general-x4v3`.

    Transcribed from upstream `realesrgan/archs/srvgg_arch.py`. The layer ordering matters: the
    checkpoint keys are positional (`body.0`, `body.2`, ...), so conv/activation pairs must be
    appended in the same order as upstream for `load_state_dict(strict=True)` to succeed.
    """

    def __init__(self, num_in_ch=3, num_out_ch=3, num_feat=64, num_conv=32, upscale=4):
        super().__init__()
        self.upscale = upscale

        self.body = nn.ModuleList()
        # the first conv
        self.body.append(nn.Conv2d(num_in_ch, num_feat, 3, 1, 1))
        # the first activation
        self.body.append(nn.PReLU(num_parameters=num_feat))

        # the body structure
        for _ in range(num_conv):
            self.body.append(nn.Conv2d(num_feat, num_feat, 3, 1, 1))
            self.body.append(nn.PReLU(num_parameters=num_feat))

        # the last conv, widened so pixel shuffle can fold channels back into resolution
        self.body.append(nn.Conv2d(num_feat, num_out_ch * upscale * upscale, 3, 1, 1))

    def forward(self, x):
        out = x
        for layer in self.body:
            out = layer(out)

        out = F.pixel_shuffle(out, self.upscale)

        # Add the nearest-neighbour upsampled input, so the network only learns the residual.
        # Upstream uses 'nearest' here; anything else changes the output.
        base = F.interpolate(x, scale_factor=self.upscale, mode="nearest")
        return out + base


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def resolve_weights(path: Path) -> Path:
    """Return a local `.pth`, downloading the upstream release asset if it is missing."""
    if not path.exists():
        print(f"Downloading {WEIGHTS_URL}")
        path.parent.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(WEIGHTS_URL) as response:
            path.write_bytes(response.read())

    digest = sha256(path.read_bytes())
    if digest != WEIGHTS_SHA256:
        print(
            f"warning: {path.name} has SHA-256 {digest}, expected {WEIGHTS_SHA256} for\n"
            f"         realesr-general-x4v3. Continuing, but the output will not match the\n"
            f"         checked-in asset.",
            file=sys.stderr,
        )
    return path


def load_state_dict(path: Path) -> dict:
    """Load a Real-ESRGAN checkpoint, unwrapping the EMA/plain weight containers it may use."""
    checkpoint = torch.load(path, map_location="cpu", weights_only=True)
    for key in ("params_ema", "params"):
        if isinstance(checkpoint, dict) and key in checkpoint:
            return checkpoint[key]
    return checkpoint


def build_model(state_dict: dict) -> SRVGGNetCompact:
    """Infer the architecture from the checkpoint rather than trusting a flag.

    `body` holds 1 conv + 1 activation, then `num_conv` conv/activation pairs, then the final
    conv: 2 * num_conv + 3 modules with indices 0..2 * num_conv + 2. The feature width and the
    upscale factor fall out of the first and last conv shapes.
    """
    indices = [int(k.split(".")[1]) for k in state_dict if k.startswith("body.")]
    if not indices:
        raise SystemExit("checkpoint has no 'body.*' keys; not an SRVGGNetCompact model")

    last = max(indices)
    num_conv = (last - 2) // 2
    num_feat = state_dict["body.0.weight"].shape[0]
    num_in_ch = state_dict["body.0.weight"].shape[1]
    num_out_ch = 3
    upscale = round((state_dict[f"body.{last}.weight"].shape[0] / num_out_ch) ** 0.5)

    print(f"SRVGGNetCompact: num_conv={num_conv} num_feat={num_feat} upscale={upscale}")
    if upscale != EXPECTED_SCALE:
        raise SystemExit(
            f"checkpoint is {upscale}x, but the upscaler hardcodes SCALE={EXPECTED_SCALE} "
            f"(proxynexus-core/src/upscaler.rs)"
        )

    model = SRVGGNetCompact(num_in_ch, num_out_ch, num_feat, num_conv, upscale)
    model.load_state_dict(state_dict, strict=True)
    model.eval()
    return model


def export(model: SRVGGNetCompact) -> bytes:
    """Export to a self-contained, byte-reproducible ONNX model."""
    height = torch.export.Dim("height")
    width = torch.export.Dim("width")

    # Exporting into a buffer keeps the weights inside the single ONNX file. A path-based export
    # can spill them to a sidecar `.onnx.data`, which burn's importer does not read.
    buffer = io.BytesIO()
    print(f"Exporting to ONNX (opset {OPSET})...")
    torch.onnx.export(
        model,
        # Values are irrelevant (the graph is shape-polymorphic), but a constant input keeps the
        # export deterministic.
        (torch.zeros(1, 3, 64, 64),),
        buffer,
        opset_version=OPSET,
        input_names=["image"],
        output_names=["output"],
        dynamic_shapes={"x": {2: height, 3: width}},
    )

    proto = onnx.load_from_string(buffer.getvalue())

    # Torch tags every node with its stack trace, which bakes absolute paths of this script and
    # of the interpreter into the file. Stripping the debug metadata is what makes the output
    # identical across machines; burn ignores it either way.
    for node in proto.graph.node:
        del node.metadata_props[:]
        node.doc_string = ""

    return proto.SerializeToString(deterministic=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "input",
        nargs="?",
        type=Path,
        default=DEFAULT_WEIGHTS,
        help=f"Real-ESRGAN .pth checkpoint (default: {DEFAULT_WEIGHTS}, downloaded if absent)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help=f"where to write the .onnx (default: {DEFAULT_OUTPUT})",
    )
    args = parser.parse_args()

    weights = resolve_weights(args.input)
    print(f"Loading {weights}...")
    model = build_model(load_state_dict(weights))

    data = export(model)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(data)

    print(f"Wrote {args.output} ({len(data) / (1024 * 1024):.2f} MB)")
    print(f"SHA-256 {sha256(data)}")


if __name__ == "__main__":
    main()
