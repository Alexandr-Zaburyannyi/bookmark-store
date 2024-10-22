/** @format */

import { Card, Flex, Pagination, Stack } from '@mantine/core';
import useBookmarkStore from '../store/store';
import { usePagination } from '@mantine/hooks';
import { memo, useCallback, useMemo } from 'react'; // Import useMemo

const BookmarkList = () => {
  const bookmarks = useBookmarkStore((state) => state.bookmarks);

  const totalItems = bookmarks.length;
  const itemsPerPage = 5;

  const pagination = usePagination({
    total: Math.ceil(totalItems / itemsPerPage),
    initialPage: 1,
  });

  const start = (pagination.active - 1) * itemsPerPage;
  const end = start + itemsPerPage;

  const currentBookmarks = useMemo(() => {
    return bookmarks.slice(start, end);
  }, [bookmarks, start, end]);

  const handlePageChange = useCallback(
    (page) => {
      pagination.setPage(page);
    },
    [pagination]
  );
  const memoizedFlex = useMemo(
    () => (
      <Flex
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
          {currentBookmarks.map((bookmark) => (
            <Card
              my='xs'
              key={bookmark.id}
              shadow='sm'
              padding='lg'
              radius='md'
              withBorder
              sx={{
                flexShrink: 0,
              }}
            >
              {bookmark.name}
            </Card>
          ))}
        </Stack>
        <Pagination
          total={Math.ceil(totalItems / itemsPerPage)}
          page={pagination.active}
          onChange={handlePageChange}
        />
      </Flex>
    ),
    [currentBookmarks, pagination.active]
  );

  return memoizedFlex;
};

export default memo(BookmarkList);
