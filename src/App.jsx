/** @format */
import { useDisclosure } from '@mantine/hooks';
import { Button, Flex, Modal, Title } from '@mantine/core';
import BookmarkForm from './components/BookmarkForm';
import useBookmarkStore from './store/store.js';

const App = () => {
  const [opened, { open, close }] = useDisclosure(false);
  const bookmarks = useBookmarkStore((state) => state);
  console.log(bookmarks);
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
