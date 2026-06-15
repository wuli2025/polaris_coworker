# -*- coding: utf-8 -*-
"""Expert avatar extraction script - extracts face crops from grid image"""
from PIL import Image
import numpy as np
import os

def is_skin(r, g, b):
    return r > 80 and g > 50 and b > 40 and max(r,g,b) > 100 and abs(r-g) < 50 and r > b

def extract_avatars(src_path, output_dir):
    """Extract expert avatars from grid image"""
    img = Image.open(src_path)
    arr = np.array(img)
    w, h = img.size
    print(f'Image size: {w}x{h}')

    # Grid parameters (derived from analysis)
    cell_w = 115
    cell_h = 115
    cols = 10
    gap = 9

    os.makedirs(output_dir, exist_ok=True)

    # Extract all high-variance cells (with content)
    face_crops = []
    for row in range(10):
        for col in range(10):
            x = gap + col * (cell_w + gap)
            y = gap + row * (cell_h + gap)
            if x + cell_w <= w and y + cell_h <= h:
                cell = arr[y:y+cell_h, x:x+cell_w, :]
                var = cell.var()

                # Check for skin tone (face)
                center_region = arr[max(0, y+cell_h//2-30):y+cell_h//2+30,
                                   max(0, x+cell_w//2-30):x+cell_w//2+30]
                skin_count = 0
                for py in range(center_region.shape[0]):
                    for px in range(center_region.shape[1]):
                        r, g, b = center_region[py, px]
                        if is_skin(r, g, b):
                            skin_count += 1

                total_center = center_region.shape[0] * center_region.shape[1]
                skin_ratio = skin_count / total_center if total_center > 0 else 0

                if skin_ratio > 0.05 or var > 3000:
                    crop = img.crop((x, y, x+cell_w, y+cell_h))
                    face_crops.append((row, col, crop, var, skin_ratio))

    print(f'Found {len(face_crops)} potential expert photos')

    # Sort by row and column
    face_crops.sort(key=lambda x: (x[0], x[1]))

    # Save with sequential names (manual renaming required after OCR verification)
    for i, (row, col, crop, var, skin_ratio) in enumerate(face_crops):
        fname = f'expert_{row:02d}_{col:02d}_{int(var)}.png'
        crop.save(os.path.join(output_dir, fname))

    print(f'Saved {len(face_crops)} crops to {output_dir}')
    return len(face_crops)

if __name__ == '__main__':
    src = r'c:\Users\mi\Desktop\专家名字.png'
    out = r'D:\polaris\polaris-app\src-tauri\src\templates\experts\avatars'
    extract_avatars(src, out)