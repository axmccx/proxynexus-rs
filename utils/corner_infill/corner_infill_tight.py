import os
import cv2
import argparse
import numpy as np
from corner_infill import save_image

WHITE_CUTOFF = 245      # Pixel intensity to consider as "blank scanner background"
INPAINT_RADIUS = 3      # Radius for the Navier-Stokes inpainting algorithm

# A card's corner radius is ~0.125 inches across a 2.48 inch width, so each
# rounded corner fits inside a square of 5% of the short side. Nothing outside
# those four squares is masked, before or after dilation, which is what keeps
# pale art along the edges out of the inpainting.
CORNER_FRACTION = 0.05

# Grown as a fraction of the scan's short side rather than a fixed pixel count,
# because the scans come in two resolutions (~1480px and ~2950px wide).
DILATE_FRACTION = 0.004


def corner_squares(height, width):
    size = max(4, int(min(height, width) * CORNER_FRACTION))
    squares = np.zeros((height, width), dtype=np.uint8)
    squares[:size, :size] = 1
    squares[:size, -size:] = 1
    squares[-size:, :size] = 1
    squares[-size:, -size:] = 1
    return squares


def build_background_mask(img):
    height, width, _ = img.shape

    white = np.any(img > WHITE_CUTOFF, axis=2).astype(np.uint8)

    # Keep only regions joined to the image border, so bright areas enclosed by
    # card art are never treated as scanner background.
    _, labels = cv2.connectedComponents(white, connectivity=8)
    seeds = set(labels[0, :]) | set(labels[-1, :]) | set(labels[:, 0]) | set(labels[:, -1])
    seeds.discard(0)
    if not seeds:
        return np.zeros((height, width), dtype=np.uint8)
    mask = np.isin(labels, list(seeds)).astype(np.uint8)

    squares = corner_squares(height, width)
    mask *= squares

    grow = max(2, int(min(height, width) * DILATE_FRACTION))
    kernel = np.ones((3, 3), np.uint8)
    mask = cv2.dilate(mask, kernel, iterations=grow) * squares

    return mask * 255


def process_corners(img):
    mask = build_background_mask(img)
    return cv2.inpaint(img, mask, INPAINT_RADIUS, cv2.INPAINT_NS)


def main():
    parser = argparse.ArgumentParser(description="Fill in the blank corners of physical card scans, touching the corners only.")
    parser.add_argument("input", help="Path to a single image or directory of images.")
    parser.add_argument("-o", "--output", help="Optional: Output directory to save processed images. Defaults to '<input>-infilled'.")

    args = parser.parse_args()

    input_path = os.path.abspath(args.input)
    if args.output:
        output_dir = args.output
    else:
        clean_input_path = input_path.rstrip(os.sep)
        output_dir = f"{clean_input_path}-infilled"

    os.makedirs(output_dir, exist_ok=True)

    if os.path.isfile(args.input):
        files = [args.input]
    elif os.path.isdir(args.input):
        files = [os.path.join(args.input, f) for f in os.listdir(args.input)
                 if f.lower().endswith(('.png', '.jpg', '.jpeg'))]
    else:
        print("Error: Input path is invalid.")
        return

    print(f"Processing {len(files)} image(s)...")

    for file_path in files:
        img = cv2.imread(file_path, cv2.IMREAD_COLOR)
        if img is None:
            print(f"Skipping {file_path} (could not read file).")
            continue

        processed_img = process_corners(img)

        filename = os.path.basename(file_path)
        out_path = os.path.join(output_dir, filename)

        save_image(processed_img, out_path, file_path)
        print(f"Saved: {out_path}")

    print("Corner infill complete.")


if __name__ == '__main__':
    main()
