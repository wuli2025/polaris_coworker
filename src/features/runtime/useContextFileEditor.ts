import { ref } from 'vue';
import { contextFiles, type ContextSelection } from './contracts';

export function useContextFileEditor() {
  const projectId = ref('');
  const createFile = ref<ContextSelection['fileName']>('AGENTS.md');
  const selection = ref<ContextSelection | null>(null);
  const content = ref('');
  const original = ref('');
  const busy = ref(false);
  const error = ref('');
  const saved = ref(false);

  async function load() {
    busy.value = true;
    error.value = '';
    saved.value = false;
    selection.value = null;
    content.value = original.value = '';
    try {
      const selected = await contextFiles.metadata(projectId.value, createFile.value);
      const text = selected.exists ? await contextFiles.read(projectId.value, selected) : '';
      loadedProject.value = projectId.value;
      selection.value = selected;
      content.value = original.value = text;
    } catch (e) { error.value = String(e); }
    finally { busy.value = false; }
  }
  // Bind edits to the loaded project, so changing the dropdown cannot redirect a save.
  const loadedProject = ref('');
  async function open() {
    if (busy.value) return;
    if (content.value !== original.value && !confirm('重新读取会丢失未保存修改，继续？')) return;
    await load();
  }
  async function switchProject(next: string): Promise<boolean> {
    if (busy.value) return false;
    if (next === projectId.value) return true;
    if (content.value !== original.value && !confirm('切换项目会丢失未保存修改，继续？')) return false;
    projectId.value = next;
    await load();
    return true;
  }
  async function save() {
    const selected = selection.value;
    if (!selected || busy.value) return;
    busy.value = true;
    error.value = '';
    saved.value = false;
    try {
      await contextFiles.write(loadedProject.value, selected, content.value);
      original.value = content.value;
      selection.value = { ...selected, exists: true,
        agentsExists: selected.agentsExists || selected.fileName === 'AGENTS.md',
        claudeExists: selected.claudeExists || selected.fileName === 'CLAUDE.md' };
      saved.value = true;
    } catch (e) { error.value = String(e); }
    finally { busy.value = false; }
  }
  return { projectId, createFile, selection, content, busy, error, saved, open, save, switchProject };
}
