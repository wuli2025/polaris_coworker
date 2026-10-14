import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { homedir } from "node:os";

export function expandPath(path: string): string {
  return resolve(path.startsWith("~/") ? join(homedir(), path.slice(2)) : path);
}
export function contextFiles(cwd: string, base: { agentsFiles: { path: string; content: string }[] }) {
  const files = new Map(base.agentsFiles.map((file) => [dirname(file.path), file]));
  const parents: string[] = [];
  for (let dir = resolve(cwd); ; dir = dirname(dir)) {
    parents.unshift(dir);
    if (dirname(dir) === dir) break;
  }
  for (const dir of parents) {
    const path = ["AGENTS.md", "CLAUDE.md"].map((name) => join(dir, name)).find(existsSync);
    if (path) files.set(dir, { path, content: readFileSync(path, "utf8") });
  }
  return { agentsFiles: [...files.values()] };
}
