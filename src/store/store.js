import { create } from 'zustand';

const useBookmarkStore = create((set) => ({
  bookmarks: [],
  setBookmarks: (bookmarks) => set(() => ({ bookmarks })),

  addBookmark: (bookmark) =>
    set((state) => ({ bookmarks: [...state.bookmarks, bookmark] })),

  removeBookmark: (bookmarkId) =>
    set((state) => ({
      bookmarks: state.bookmarks.filter((item) => item.id !== bookmarkId),
    })),

  sources: [],
  setSources: (sources) => set(() => ({ sources })),

  syncing: false,
  setSyncing: (syncing) => set(() => ({ syncing })),

  lastSync: null,
  setLastSync: (lastSync) => set(() => ({ lastSync })),

  folders: [],
  setFolders: (folders) => set(() => ({ folders })),

  selectedFolder: 'all',
  setSelectedFolder: (selectedFolder) => set(() => ({ selectedFolder })),

  statusFilter: 'all',
  setStatusFilter: (statusFilter) => set(() => ({ statusFilter })),
}));

export default useBookmarkStore;
