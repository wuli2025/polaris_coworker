import { invoke } from '../../tauri';

export interface ContextSelection {
  root: string;
  fileName: 'AGENTS.md' | 'CLAUDE.md';
  relativePath: string;
  agentsExists: boolean;
  claudeExists: boolean;
  exists: boolean;
}
export const contextFiles = {
  metadata: (projectId: string, createFile: ContextSelection['fileName'] = 'AGENTS.md') =>
    invoke<ContextSelection>('context_file_metadata', { projectId, createFile }),
  read: (projectId: string, selection: ContextSelection) =>
    invoke<string>('context_file_read', { projectId, selection }),
  write: (projectId: string, selection: ContextSelection, content: string) =>
    invoke<void>('context_file_write', { projectId, selection, content }),
};
export interface ExpertState { enabled: boolean }
export const expertSettings = {
  get: () => invoke<ExpertState>('expert_enabled_get'),
  set: (enabled: boolean) => invoke<ExpertState>('expert_enabled_set', { enabled }),
};
