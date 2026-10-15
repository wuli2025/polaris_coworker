import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useContextFileEditor } from '../../src/features/runtime/useContextFileEditor';

const ports = vi.hoisted(() => ({ metadata: vi.fn(), read: vi.fn(), write: vi.fn() }));
vi.mock('../../src/features/runtime/contracts', () => ({ contextFiles: ports }));
const confirm = vi.fn();
beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal('confirm', confirm);
  ports.metadata.mockImplementation(async (id: string) => ({
    root: `/${id}`, fileName: 'AGENTS.md', relativePath: 'AGENTS.md',
    exists: true, agentsExists: true, claudeExists: false,
  }));
  ports.read.mockImplementation(async (id: string) => `content:${id}`);
});
afterEach(() => vi.unstubAllGlobals());

async function loadedEditor() {
  const editor = useContextFileEditor();
  await editor.switchProject('a');
  return editor;
}

it('cancels a dirty project switch, retaining selection, edits and save destination', async () => {
  const editor = await loadedEditor();
  editor.content.value = 'unsaved';
  const selected = editor.selection.value;
  confirm.mockReturnValue(false);
  expect(await editor.switchProject('b')).toBe(false);
  expect(confirm).toHaveBeenCalledOnce();
  expect(editor.projectId.value).toBe('a');
  expect(editor.selection.value).toBe(selected);
  expect(editor.content.value).toBe('unsaved');
  expect(ports.metadata).toHaveBeenCalledTimes(1);
  await editor.save();
  expect(ports.write).toHaveBeenCalledWith('a', selected, 'unsaved');
});

it('confirms once before reading the new project and binds its saves', async () => {
  const editor = await loadedEditor();
  editor.content.value = 'unsaved';
  confirm.mockImplementation(() => {
    expect(editor.projectId.value).toBe('a');
    expect(ports.metadata).toHaveBeenCalledTimes(1);
    return true;
  });
  expect(await editor.switchProject('b')).toBe(true);
  expect(confirm).toHaveBeenCalledOnce();
  expect(editor.projectId.value).toBe('b');
  expect(editor.content.value).toBe('content:b');
  editor.content.value = 'new edit';
  await editor.save();
  expect(ports.write).toHaveBeenCalledWith('b', editor.selection.value, 'new edit');
});

it('switches clean projects without prompting and ignores the current project', async () => {
  const editor = await loadedEditor();
  await editor.switchProject('a');
  expect(ports.metadata).toHaveBeenCalledTimes(1);
  await editor.switchProject('b');
  expect(confirm).not.toHaveBeenCalled();
  expect(editor.content.value).toBe('content:b');
});

it('does not allow another switch during loading or retain a stale save target on read failure', async () => {
  const editor = await loadedEditor();
  ports.read.mockRejectedValueOnce(new Error('read denied'));
  const pending = editor.switchProject('b');
  expect(await editor.switchProject('c')).toBe(false);
  await pending;
  expect(editor.projectId.value).toBe('b');
  expect(editor.selection.value).toBeNull();
  expect(editor.error.value).toContain('read denied');
  await editor.save();
  expect(ports.write).not.toHaveBeenCalled();
});
