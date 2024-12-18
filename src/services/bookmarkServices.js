/** @format */
import { invoke } from '@tauri-apps/api/tauri';
import { refactorBookmarks } from '../helpers/helpers';

export const setBookmarks = async (setBookmarks) => {
  const bookmarks = await invoke('get_bookmarks');
  setBookmarks(refactorBookmarks(bookmarks));
};
