/** @format */
import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { useDisclosure } from '@mantine/hooks';
import { Button, Flex, Modal, Title } from '@mantine/core';
import BookmarkForm from './components/BookmarkForm';
import useBookmarkStore from './store/store.js';

const App = () => {
  const [opened, { open, close }] = useDisclosure(false);
  const bookmarks = useBookmarkStore((state) => state);

  useEffect(() => {
    const getBookmarks = async () => {
      try {
        await invoke('delete_bookmark', { bookmarkId: 10 });
        const bookmarks = await invoke('get_bookmarks');
        console.log(bookmarks);
      } catch (error) {
        console.log(error);
      }
    };
    getBookmarks();
  }, []);

  return (
    <>
      <Modal
        title='Add bookmark'
        opened={opened}
        onClose={close}
      >
        <BookmarkForm />
      </Modal>
      <Flex
        mt='md'
        w='100%'
        justify='center'
        direction='column'
        align='center'
      >
        <Title
          order={2}
          m='auto'
        >
          Welcome to your bookmark store!
        </Title>
        <Button onClick={open}>Add bookmark</Button>
      </Flex>
    </>
  );
};

export default App;
