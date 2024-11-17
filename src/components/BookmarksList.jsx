/** @format */
import { UnstyledButton, Card, Title } from '@mantine/core';
import { memo } from 'react';
const BookmarksList = ({ bookmarks }) => {
  return bookmarks.map((bookmark) => (
    <UnstyledButton key={bookmark.id}>
      {memo(
        <Card
          my='xs'
          key={bookmark.id}
          shadow='sm'
          padding='lg'
          radius='md'
          withBorder
          display='flex'
        >
          <Title
            order={4}
            tt='capitalize'
          >
            {bookmark.name}
          </Title>
        </Card>
      )}
    </UnstyledButton>
  ));
};
export default memo(BookmarksList);
