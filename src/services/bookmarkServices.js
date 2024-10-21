/** @format */
import { invoke } from '@tauri-apps/api/tauri';

export const setBookmarks = async (setBookmarks) => {
  const bookmarks = await invoke('get_bookmarks');
  setBookmarks(bookmarks);
};
