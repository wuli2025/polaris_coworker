/** Existing claude_md_* endpoints cannot choose AGENTS.md or report its existence. */
export function contextEditingAvailable(backend: string | undefined): boolean {
  return backend === 'claude-cli';
}
