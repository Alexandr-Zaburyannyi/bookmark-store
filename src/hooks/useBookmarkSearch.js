/** @format */
import { useMemo } from 'react';
import Fuse from 'fuse.js';

const FUSE_OPTIONS = {
  keys: [
    { name: 'name', weight: 3 },
    { name: 'link', weight: 2 },
    { name: 'description', weight: 1 },
  ],
  threshold: 0.35,
  ignoreLocation: true,
  minMatchCharLength: 2,
};

const useBookmarkSearch = (bookmarks, query) => {
  const fuse = useMemo(() => new Fuse(bookmarks, FUSE_OPTIONS), [bookmarks]);

  return useMemo(() => {
    const trimmed = query.trim();
    if (trimmed.length < 2) {
      return bookmarks;
    }
    return fuse.search(trimmed).map((result) => result.item);
  }, [fuse, bookmarks, query]);
};

export default useBookmarkSearch;
