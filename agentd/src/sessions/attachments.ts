import type { Config } from "../config/schema.ts";
import { DaemonError } from "../protocol/errors.ts";
import type { ImageAttachment } from "../protocol/schema.ts";

/** Called before schema validation too, to give unsupported kinds a named error. */
export function validateAttachments(value: unknown, config: Config): void {
  if (!Array.isArray(value)) return; // The wire schema rejects non-arrays.
  const limits = config.attachments;
  if (value.length > limits.maxCount) throw new DaemonError("ATTACHMENT_COUNT_LIMIT", "Too many images");
  let total = 0;
  for (const item of value) {
    if (!item || item.type !== "image" || !limits.mimeTypes.includes(item.mimeType)) {
      throw new DaemonError("ATTACHMENT_TYPE_UNSUPPORTED", "Unsupported image attachment type");
    }
    if (typeof item.data !== "string") continue; // Schema owns field types.
    if (item.data.length > Math.ceil(limits.maxBytes / 3) * 4) {
      throw new DaemonError("ATTACHMENT_SIZE_LIMIT", "Image exceeds configured byte limit");
    }
    const bytes = Buffer.from(item.data, "base64");
    if (!bytes.length || bytes.toString("base64") !== item.data) {
      throw new DaemonError("ATTACHMENT_INVALID_BASE64", "Image must contain canonical base64");
    }
    total += bytes.length;
    if (bytes.length > limits.maxBytes || total > limits.maxTotalBytes) {
      throw new DaemonError("ATTACHMENT_SIZE_LIMIT", "Images exceed configured byte limit");
    }
  }
}
export function imagesForPrompt(images: ImageAttachment[], config: Config): ImageAttachment[] {
  validateAttachments(images, config);
  return images;
}
