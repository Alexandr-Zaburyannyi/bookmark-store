import {
  ActionIcon,
  Badge,
  Button,
  Card,
  Group,
  Menu,
  Select,
  Stack,
  Text,
  Title,
} from '@mantine/core';
import { modals } from '@mantine/modals';
import { memo, useMemo, useState } from 'react';
import useBookmarkStore from '../store/store';
import { AnimatePresence, motion } from 'framer-motion';
import { cardMotionStyle } from '../config/config';
import {
  bookmarkStatus,
  displayHost,
  folderOptions,
  groupSources,
} from '../helpers/helpers';
import { deleteBookmark, moveBookmark } from '../services/bookmarkServices';
import SourceBadges from './SourceBadges';

const MoveToFolderForm = ({ bookmark }) => {
  const folders = useBookmarkStore((state) => state.folders);
  const [value, setValue] = useState(
    bookmark.folderId == null ? 'unfiled' : String(bookmark.folderId)
  );

  const data = useMemo(
    () => [{ value: 'unfiled', label: 'Unfiled' }, ...folderOptions(folders)],
    [folders]
  );

  const handleMove = async () => {
    modals.closeAll();
    await moveBookmark(bookmark.id, value === 'unfiled' ? null : Number(value));
  };

  return (
    <Stack>
      <Select
        label='Folder'
        data={data}
        value={value}
        onChange={setValue}
        allowDeselect={false}
        comboboxProps={{ withinPortal: false }}
      />
      <Button onClick={handleMove}>Move</Button>
    </Stack>
  );
};

const openMoveModal = (bookmark) => {
  modals.open({
    title: 'Move to folder',
    centered: true,
    children: <MoveToFolderForm bookmark={bookmark} />,
  });
};

const confirmDelete = (bookmark) => {
  const browsers = groupSources(bookmark.sources ?? [])
    .filter((source) => source.present)
    .map((source) => source.label);

  modals.openConfirmModal({
    title: 'Delete bookmark',
    centered: true,
    children: (
      <Stack gap='xs'>
        <Text size='sm'>Delete &quot;{bookmark.name}&quot; from this app?</Text>
        {browsers.length > 0 && (
          <Text
            size='sm'
            c='dimmed'
          >
            It will also be removed from: {browsers.join(', ')} (on the next
            sync, once each browser is closed).
          </Text>
        )}
      </Stack>
    ),
    labels: { confirm: 'Delete', cancel: 'Cancel' },
    confirmProps: { color: 'red' },
    onConfirm: () => deleteBookmark(bookmark),
  });
};

const BookmarksList = ({ bookmarks }) => {
  const initialAnimation = useMemo(() => ({ opacity: 0, y: 20 }), []);

  return (
    <AnimatePresence mode='popLayout'>
      {bookmarks.map((bookmark) => (
        <motion.div
          layout
          style={cardMotionStyle}
          key={bookmark.id}
          initial={initialAnimation}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 20 }}
          whileHover={{ scale: 1.01 }}
        >
          <Card
            h='100%'
            shadow='sm'
            padding='lg'
            radius='md'
            withBorder
            display='flex'
          >
            <Stack
              gap='xs'
              style={{ flex: 1, minWidth: 0 }}
            >
              <Group
                justify='space-between'
                align='flex-start'
                wrap='nowrap'
              >
                <Title
                  order={4}
                  tt='capitalize'
                  lineClamp={2}
                >
                  {bookmark.name}
                </Title>
                <Menu
                  position='bottom-end'
                  withinPortal
                >
                  <Menu.Target>
                    <ActionIcon
                      variant='subtle'
                      color='gray'
                      aria-label='Bookmark actions'
                    >
                      ⋯
                    </ActionIcon>
                  </Menu.Target>
                  <Menu.Dropdown>
                    <Menu.Item onClick={() => openMoveModal(bookmark)}>
                      Move to folder
                    </Menu.Item>
                    <Menu.Item
                      color='red'
                      onClick={() => confirmDelete(bookmark)}
                    >
                      Delete
                    </Menu.Item>
                  </Menu.Dropdown>
                </Menu>
              </Group>
              <Text
                size='xs'
                c='dimmed'
                truncate='end'
              >
                {displayHost(bookmark.link)}
              </Text>
              {bookmarkStatus(bookmark) === 'removed' && (
                <Badge
                  color='red'
                  variant='light'
                  w='fit-content'
                >
                  Removed from browsers
                </Badge>
              )}
              <SourceBadges sources={bookmark.sources ?? []} />
            </Stack>
          </Card>
        </motion.div>
      ))}
    </AnimatePresence>
  );
};
export default memo(BookmarksList);
