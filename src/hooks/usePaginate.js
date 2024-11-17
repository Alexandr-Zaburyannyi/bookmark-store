/** @format */

import { usePagination } from '@mantine/hooks';
import { useMemo } from 'react';
const usePaginate = (totalItems, itemsPerPage, bookmarks) => {
  const pagination = usePagination({
    total: Math.ceil(totalItems / itemsPerPage),
    initialPage: 1,
  });
  const start = (pagination.active - 1) * itemsPerPage;
  const end = start + itemsPerPage;

  const currentBookmarks = useMemo(() => {
    return bookmarks.slice(start, end);
  }, [start, end, bookmarks]);

  const totalPages = Math.ceil(totalItems / itemsPerPage);
  return { pagination, currentBookmarks, totalPages };
};
export default usePaginate;
