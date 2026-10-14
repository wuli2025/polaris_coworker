import type { AttachedFile } from "../tauri";

/** Preserve every uploaded image for backend validation, including unsupported MIME types. */
export function uploadedImagePaths(files?: readonly AttachedFile[]): string[] | undefined {
  const paths = files?.filter((file) => file.ok && file.kind === "image").map((file) => file.path);
  return paths?.length ? paths : undefined;
}
