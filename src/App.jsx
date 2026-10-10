import { useLayoutEffect, useMemo } from 'react';
import { useDisclosure } from '@mantine/hooks';
import { AppShell, Button, Flex, Group, Modal, Title } from '@mantine/core';
import BookmarkForm from './components/BookmarkForm';
import useBookmarkStore from './store/store.js';
import {
  loadFolders,
  setBookmarks,
  syncBookmarks,
} from './services/bookmarkServices.js';
import BookmarkPage from './components/Bookmark.jsx';
import SourcesDrawer from './components/SourcesDrawer.jsx';
import SyncStatus from './components/SyncStatus.jsx';
import FolderSidebar from './components/FolderSidebar.jsx';

const App = () => {
  const [opened, { open, close }] = useDisclosure(false);
  const [sourcesOpened, { open: openSources, close: closeSources }] =
    useDisclosure(false);

  const setBookmarksStore = useBookmarkStore((state) => state.setBookmarks);
  const syncing = useBookmarkStore((state) => state.syncing);

  useLayoutEffect(() => {
    setBookmarks(setBookmarksStore).then(syncBookmarks);
    loadFolders();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const memoizedButtons = useMemo(
    () => (
      <Group>
        <Button onClick={open}>Add bookmark</Button>
        <Button
          variant='light'
          onClick={openSources}
        >
          Sources
        </Button>
        <Button
          variant='outline'
          loading={syncing}
          onClick={syncBookmarks}
        >
          Sync now
        </Button>
        <SyncStatus />
      </Group>
    ),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [syncing]
  );

  const memoizedFlex = useMemo(
    () => (
      <Flex
        h='100%'
        w='100%'
        justify='space-evenly'
        align='center'
      >
        <Title order={2}>Welcome to your bookmark store!</Title>
        {memoizedButtons}
      </Flex>
    ),
    [memoizedButtons]
  );

  return (
    <>
      {useMemo(
        () => (
          <Modal
            title='Add bookmark'
            opened={opened}
            onClose={close}
            radius={'md'}
          >
            <BookmarkForm onDone={close} />
          </Modal>
        ),
        [opened, close]
      )}
      <SourcesDrawer
        opened={sourcesOpened}
        onClose={closeSources}
      />
      <AppShell
        padding={0}
        header={{ height: 70 }}
        navbar={{ width: 240, breakpoint: 'sm' }}
      >
        <AppShell.Header>{memoizedFlex}</AppShell.Header>
        <AppShell.Navbar p='md'>
          <FolderSidebar />
        </AppShell.Navbar>
        <AppShell.Main
          style={{ height: '100vh', display: 'flex', flexDirection: 'column' }}
        >
          <BookmarkPage />
        </AppShell.Main>
      </AppShell>
    </>
  );
};

export default App;
