/** @format */

import { useDisclosure } from '@mantine/hooks';
import { Button, Flex, Modal, Title } from '@mantine/core';
import BookmarkForm from './components/BookmarkForm';
import useBookmarkStore from './store/store.js';
import { setBookmarks } from './services/bookmarkServices.js';
import { useLayoutEffect, useMemo } from 'react';
import BookmarkPage from './components/Bookmark.jsx';

const App = () => {
  const [opened, { open, close }] = useDisclosure(false);

  const setBookmarksStore = useBookmarkStore((state) => state.setBookmarks);

  useLayoutEffect(() => {
    setBookmarks(setBookmarksStore);
  }, []);

  const memoizedButton = useMemo(
    () => <Button onClick={open}>Add bookmark</Button>,
    []
  );

  const memoizedFlex = useMemo(
    () => (
      <Flex
        mt='md'
        w='100%'
        justify='space-evenly'
        align='center'
      >
        <Title order={2}>Welcome to your bookmark store!</Title>
        {memoizedButton}
      </Flex>
    ),
    []
  );

  return (
    <>
      {useMemo(
        () => (
          <Modal
            title='Add bookmark'
            opened={opened}
            onClose={close}
          >
            <BookmarkForm />
          </Modal>
        ),
        [opened, close]
      )}
      {memoizedFlex}
      <BookmarkPage />
    </>
  );
};

export default App;
