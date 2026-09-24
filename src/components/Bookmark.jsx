import { Flex, Pagination, SegmentedControl, SimpleGrid } from '@mantine/core';
import { memo, useCallback, useEffect, useMemo } from 'react';
import { useViewportSize } from '@mantine/hooks';
import useBookmarkStore from '../store/store';
import usePaginate from '../hooks/usePaginate';
import { bookmarkStatus } from '../helpers/helpers';
import BookmarksList from './BookmarksList';

const STATUS_FILTERS = [
  { label: 'All', status: 'all' },
  { label: 'In browsers', status: 'synced' },
  { label: 'Removed', status: 'removed' },
  { label: 'Added here', status: 'manual' },
];

const COLUMN_WIDTH = 280;
const MAX_COLUMNS = 4;
const ROW_PITCH = 150;
const NAVBAR_WIDTH = 240;
const CHROME_HEIGHT = 250;

const BookmarkPage = () => {
  const { height: viewportHeight, width: viewportWidth } = useViewportSize();

  const bookmarks = useBookmarkStore((state) => state.bookmarks);
  const selectedFolder = useBookmarkStore((state) => state.selectedFolder);
  const statusFilter = useBookmarkStore((state) => state.statusFilter);
  const setStatusFilter = useBookmarkStore((state) => state.setStatusFilter);

  const folderBookmarks = useMemo(() => {
    if (selectedFolder === 'all') {
      return bookmarks;
    }
    if (selectedFolder === 'unfiled') {
      return bookmarks.filter((bookmark) => bookmark.folderId == null);
    }
    return bookmarks.filter((bookmark) => bookmark.folderId === selectedFolder);
  }, [bookmarks, selectedFolder]);

  const counts = useMemo(() => {
    const counts = {
      all: folderBookmarks.length,
      synced: 0,
      removed: 0,
      manual: 0,
    };
    for (const bookmark of folderBookmarks) {
      counts[bookmarkStatus(bookmark)] += 1;
    }
    return counts;
  }, [folderBookmarks]);

  const filteredBookmarks = useMemo(() => {
    if (statusFilter === 'all') {
      return folderBookmarks;
    }
    return folderBookmarks.filter(
      (bookmark) => bookmarkStatus(bookmark) === statusFilter
    );
  }, [folderBookmarks, statusFilter]);

  const totalItems = filteredBookmarks.length;
  const columns = Math.min(
    MAX_COLUMNS,
    Math.max(1, Math.floor((viewportWidth - NAVBAR_WIDTH) / COLUMN_WIDTH))
  );
  const rows = Math.max(
    1,
    Math.floor((viewportHeight - CHROME_HEIGHT) / ROW_PITCH)
  );
  const itemsPerPage = columns * rows;

  const { pagination, currentBookmarks, totalPages } = usePaginate(
    totalItems,
    itemsPerPage,
    filteredBookmarks
  );

  useEffect(() => {
    if (totalPages > 0) {
      pagination.setPage(1);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [statusFilter, selectedFolder]);

  const handlePageChange = useCallback(
    (page) => {
      pagination.setPage(page);
    },
    [pagination]
  );

  const segmentedData = useMemo(
    () =>
      STATUS_FILTERS.map(({ label, status }) => ({
        value: status,
        label: `${label} (${counts[status]})`,
      })),
    [counts]
  );

  const memoizedFlex = useMemo(
    () => (
      <Flex
        direction='column'
        p='md'
        style={{ flex: 1, minHeight: 0 }}
      >
        <SegmentedControl
          mb='sm'
          value={statusFilter}
          onChange={setStatusFilter}
          data={segmentedData}
        />

        <div
          style={{
            flexGrow: 1,
            flexShrink: 1,
            minHeight: 0,
            overflowY: 'auto',
          }}
        >
          <SimpleGrid
            cols={columns}
            spacing='md'
            mb='sm'
          >
            <BookmarksList bookmarks={currentBookmarks} />
          </SimpleGrid>
        </div>

        <Pagination
          total={totalPages}
          value={pagination.active}
          onChange={handlePageChange}
        />
      </Flex>
    ),
    [
      currentBookmarks,
      columns,
      pagination.active,
      totalPages,
      statusFilter,
      segmentedData,
      setStatusFilter,
      handlePageChange,
    ]
  );

  return memoizedFlex;
};

export default memo(BookmarkPage);
