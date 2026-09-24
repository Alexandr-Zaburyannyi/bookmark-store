import { invoke } from '@tauri-apps/api/tauri';
import { notifications } from '@mantine/notifications';
import useBookmarkStore from '../store/store';

export const setBookmarks = async (setBookmarks) => {
  const bookmarks = await invoke('get_bookmarks');
  setBookmarks(bookmarks);
};

export const loadFolders = async () => {
  const folders = await invoke('list_folders');
  const store = useBookmarkStore.getState();
  store.setFolders(folders);

  const { selectedFolder } = store;
  if (
    typeof selectedFolder === 'number' &&
    !folders.some((folder) => folder.id === selectedFolder)
  ) {
    store.setSelectedFolder('all');
  }
};

const runFolderAction = async (title, action) => {
  try {
    await action();
  } catch (error) {
    notifications.show({ color: 'red', title, message: String(error) });
    return false;
  }
  return true;
};

export const createFolder = (name, parentId = null) =>
  runFolderAction('Could not create folder', async () => {
    await invoke('create_folder', { name, parentId });
    await loadFolders();
  });

export const renameFolder = (folderId, name) =>
  runFolderAction('Could not rename folder', async () => {
    await invoke('rename_folder', { folderId, name });
    await loadFolders();
  });

export const deleteFolder = (folderId) =>
  runFolderAction('Could not delete folder', async () => {
    await invoke('delete_folder', { folderId });
    await setBookmarks(useBookmarkStore.getState().setBookmarks);
    await loadFolders();
  });

export const setFolderTargets = (folderId, sourceIds) =>
  runFolderAction('Could not update browsers for folder', async () => {
    await invoke('set_folder_targets', { folderId, sourceIds });
    await loadFolders();
    await loadSources();
  });

export const moveBookmark = (bookmarkId, folderId) =>
  runFolderAction('Could not move bookmark', async () => {
    await invoke('move_bookmark', { bookmarkId, folderId });
    await setBookmarks(useBookmarkStore.getState().setBookmarks);
    await loadFolders();
  });

export const loadSources = async () => {
  const sources = await invoke('list_sources');
  useBookmarkStore.getState().setSources(sources);
};

const notifySyncResult = (summary) => {
  if (summary.added > 0 || summary.markedRemoved > 0) {
    notifications.show({
      color: 'green',
      message: `Synced ${summary.sources} sources · ${summary.added} new · ${summary.markedRemoved} removed from browsers`,
    });
  }

  if (summary.failed.length > 0) {
    notifications.show({
      color: 'red',
      title: 'Some sources failed to sync',
      message: summary.failed.map((source) => source.label).join(', '),
    });
  }
};

const notifyWriteBack = (writeBack) => {
  if (!writeBack) {
    return;
  }

  const unique = (items) => [...new Set(items)];

  const updated = unique(writeBack.applied.map((source) => source.browser));
  if (updated.length > 0) {
    notifications.show({
      color: 'green',
      message: `Updated ${updated.join(', ')}`,
    });
  }

  const waiting = unique(
    writeBack.skipped
      .filter((source) => source.reason === 'browser_running')
      .map((source) => source.browser)
  );
  if (waiting.length > 0) {
    notifications.show({
      color: 'yellow',
      message: `Close ${waiting.join(', ')} to apply pending changes`,
    });
  }

  if (writeBack.failed.length > 0) {
    notifications.show({
      color: 'red',
      title: 'Could not write to some browsers',
      message: writeBack.failed
        .map((source) => `${source.label}: ${source.error}`)
        .join('\n'),
    });
  }
};

let inflight = null;

export const syncBookmarks = async () => {
  if (inflight) {
    return inflight;
  }

  const store = useBookmarkStore.getState();
  store.setSyncing(true);

  inflight = (async () => {
    try {
      const summary = await invoke('sync_bookmarks');
      await setBookmarks(store.setBookmarks);
      await loadSources();
      await loadFolders();
      store.setLastSync({ ...summary, at: Date.now() });
      notifySyncResult(summary);
      notifyWriteBack(summary.writeBack);
      return summary;
    } finally {
      store.setSyncing(false);
      inflight = null;
    }
  })();

  return inflight;
};

export const deleteBookmark = async (bookmark) => {
  const store = useBookmarkStore.getState();
  store.removeBookmark(bookmark.id);

  try {
    await invoke('delete_bookmark', { bookmarkId: bookmark.id });
    await loadFolders();
  } catch (error) {
    notifications.show({
      color: 'red',
      title: 'Could not delete bookmark',
      message: String(error),
    });
    await setBookmarks(store.setBookmarks);
  }
};
