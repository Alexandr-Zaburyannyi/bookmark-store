/** @format */

import { Flex, Pagination, Stack } from '@mantine/core';
import useBookmarkStore from '../store/store';
import { useMediaQuery } from '@mantine/hooks';
import { memo, useCallback, useMemo } from 'react'; // Import useMemo
import usePaginate from '../hooks/usePaginate';
import BookmarksList from './BookmarksList';

const BookmarkPage = () => {
  const isSmallWindow = useMediaQuery('(max-height: 600px)');
  const isBigWindow = useMediaQuery('(max-height: 900px)');

  const bookmarks = useBookmarkStore((state) => state.bookmarks);

  const totalItems = bookmarks.length;
  const itemsPerPage = isSmallWindow ? 4 : isBigWindow ? 5 : 7;

  const { pagination, currentBookmarks, totalPages } = usePaginate(
    totalItems,
    itemsPerPage,
    bookmarks
  );

  const handlePageChange = useCallback(
    (page) => {
      pagination.setPage(page);
    },
    [pagination]
  );
  const memoizedFlex = useMemo(
    () => (
      <Flex
        h='100vh'
        direction='column'
        p='md'
      >
        <Stack
          h='80vh'
          mb='sm'
          sx={{
            flexGrow: 0,
            flexShrink: 0,
          }}
        >
          <BookmarksList bookmarks={currentBookmarks} />
        </Stack>
        <Pagination
          total={totalPages}
          page={pagination.active}
          onChange={handlePageChange}
        />
      </Flex>
    ),
    [currentBookmarks, pagination.active]
  );

  return memoizedFlex;
};

export default memo(BookmarkPage);
