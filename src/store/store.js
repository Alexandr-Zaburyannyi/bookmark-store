/** @format */

import create from 'zustand';

export const useStore = create((set) => ({
  bookmarks: [],
  addBookmark: (bookmark) =>
    set((state) => ({ bookmarks: [...state.bookmarks, bookmark] })),
  removeBookmark: (bookmarkId) =>
    set((state) => ({
      bookmarks: state.bookmarks.filter((item) => {
        item.id !== bookmarkId;
      }),
    })),
}));
