/** @format */
import { useEffect } from 'react';
import useBookmarkStore from '../store/store';
import { invoke } from '@tauri-apps/api/tauri';

export const useGetBookmarks = () => {
  const setBookmarks = useBookmarkStore((state) => state.setBookmarks);

  useEffect(() => {
    (async function fetchBookmarks() {
      setBookmarks(await invoke('get_bookmarks'));
    })();
  }, []);
  return useBookmarkStore((state) => state.bookmarks);
};
