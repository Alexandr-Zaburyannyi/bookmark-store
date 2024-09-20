/** @format */

import create from 'zustand';

export const useBookmarkStore = create((set) => ({
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

export const useModalStateStore = create((set) => ({
  modalIsOpen: false,
  toggleModal: () => set((state) => (state.modalIsOpen = !state.modalIsOpen)),
}));
