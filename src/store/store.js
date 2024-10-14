/** @format */

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
}));

export default useBookmarkStore;
