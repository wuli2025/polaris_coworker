import { describe, expect, it } from "vitest";
import type { AttachedFile } from "../tauri";
import { uploadedImagePaths } from "./chatImages";

describe("uploadedImagePaths", () => {
  it("omits the optional field for old text-only sends", () => {
    expect(uploadedImagePaths()).toBeUndefined();
    expect(uploadedImagePaths([])).toBeUndefined();
  });

  it("keeps uploaded image order and paths without guessing MIME or mutating attachments", () => {
    const files: AttachedFile[] = [
      { name: "a.png", path: "/uploads/a.png", kind: "image", size: 12, ok: true },
      { name: "b.txt", path: "/uploads/b.txt", kind: "text", size: 12, ok: true },
      { name: "failed.png", path: "", kind: "image", size: 0, ok: false },
      { name: "c.svg", path: "/uploads/c.svg", kind: "image", size: 12, ok: true },
    ];
    const before = JSON.stringify(files);
    expect(uploadedImagePaths(files)).toEqual(["/uploads/a.png", "/uploads/c.svg"]);
    expect(JSON.stringify(files)).toBe(before);
  });
});
