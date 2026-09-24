/** @format */
import {
  ActionIcon,
  Badge,
  Button,
  Group,
  Menu,
  NavLink,
  Stack,
  Text,
} from '@mantine/core';
import { modals } from '@mantine/modals';
import { memo, useCallback, useMemo, useState } from 'react';
import useBookmarkStore from '../store/store';
import { buildFolderTree } from '../helpers/helpers';
import {
  createFolder,
  deleteFolder,
  renameFolder,
} from '../services/bookmarkServices';
import FolderNameModal from './FolderNameModal';
import FolderTargetsModal from './FolderTargetsModal';

const confirmDeleteFolder = (folder) => {
  modals.openConfirmModal({
    title: 'Delete folder',
    centered: true,
    children: (
      <Text size='sm'>
        Delete &quot;{folder.name}&quot;? Its bookmarks move to Unfiled, and any
        subfolders are deleted too.
      </Text>
    ),
    labels: { confirm: 'Delete', cancel: 'Cancel' },
    confirmProps: { color: 'red' },
    onConfirm: () => deleteFolder(folder.id),
  });
};

const publishedTo = (folder, browsersBySource) => {
  const browsers = [
    ...new Set(
      folder.targets
        .filter((target) => target.state === 'active')
        .map((target) => browsersBySource.get(target.sourceId))
        .filter(Boolean)
    ),
  ];
  return browsers.length > 0 ? `In: ${browsers.join(', ')}` : undefined;
};

const FolderNode = ({
  folder,
  selectedFolder,
  onSelect,
  onRename,
  onAddChild,
  onPublish,
  browsersBySource,
}) => (
  <NavLink
    component='div'
    label={folder.name}
    description={publishedTo(folder, browsersBySource)}
    active={selectedFolder === folder.id}
    defaultOpened
    childrenOffset={16}
    onClick={() => onSelect(folder.id)}
    rightSection={
      <Group
        gap={4}
        wrap='nowrap'
        onClick={(event) => event.stopPropagation()}
      >
        <Badge
          variant='light'
          color='gray'
        >
          {folder.bookmarkCount}
        </Badge>
        <Menu
          position='bottom-end'
          withinPortal
        >
          <Menu.Target>
            <ActionIcon
              variant='subtle'
              color='gray'
              size='sm'
              aria-label={`Actions for ${folder.name}`}
            >
              ⋯
            </ActionIcon>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Item onClick={() => onRename(folder)}>Rename</Menu.Item>
            {folder.parentId == null && (
              <Menu.Item onClick={() => onPublish(folder)}>
                Show in browsers…
              </Menu.Item>
            )}
            <Menu.Item onClick={() => onAddChild(folder)}>
              New subfolder
            </Menu.Item>
            <Menu.Item
              color='red'
              onClick={() => confirmDeleteFolder(folder)}
            >
              Delete
            </Menu.Item>
          </Menu.Dropdown>
        </Menu>
      </Group>
    }
  >
    {folder.children.map((child) => (
      <FolderNode
        key={child.id}
        folder={child}
        selectedFolder={selectedFolder}
        onSelect={onSelect}
        onRename={onRename}
        onAddChild={onAddChild}
        onPublish={onPublish}
        browsersBySource={browsersBySource}
      />
    ))}
  </NavLink>
);

const FolderSidebar = () => {
  const folders = useBookmarkStore((state) => state.folders);
  const bookmarks = useBookmarkStore((state) => state.bookmarks);
  const selectedFolder = useBookmarkStore((state) => state.selectedFolder);
  const setSelectedFolder = useBookmarkStore(
    (state) => state.setSelectedFolder
  );

  // { mode: 'create' | 'rename', parentId?, folder? } or null when closed
  const [editing, setEditing] = useState(null);

  const tree = useMemo(() => buildFolderTree(folders), [folders]);
  const unfiledCount = useMemo(
    () => bookmarks.filter((bookmark) => bookmark.folderId == null).length,
    [bookmarks]
  );

  const sources = useBookmarkStore((state) => state.sources);
  const [publishing, setPublishing] = useState(null);

  const browsersBySource = useMemo(
    () => new Map(sources.map((source) => [source.id, source.browser])),
    [sources]
  );
  // re-derived from the store so the modal sees fresh targets after a save
  const publishingFolder = useMemo(
    () => folders.find((folder) => folder.id === publishing) ?? null,
    [folders, publishing]
  );

  const closeModal = useCallback(() => setEditing(null), []);
  const openCreate = useCallback(
    (parent) => setEditing({ mode: 'create', parentId: parent?.id ?? null }),
    []
  );
  const openRename = useCallback(
    (folder) => setEditing({ mode: 'rename', folder }),
    []
  );

  const handleSubmit = useCallback(
    async (name) => {
      const current = editing;
      setEditing(null);
      if (current.mode === 'create') {
        await createFolder(name, current.parentId);
      } else {
        await renameFolder(current.folder.id, name);
      }
    },
    [editing]
  );

  const modalTitle =
    editing?.mode === 'rename'
      ? 'Rename folder'
      : editing?.parentId != null
        ? 'New subfolder'
        : 'New folder';

  return (
    <Stack gap='xs'>
      <Button
        variant='light'
        onClick={() => openCreate(null)}
      >
        New folder
      </Button>

      <div>
        <NavLink
          component='div'
          label='All bookmarks'
          active={selectedFolder === 'all'}
          onClick={() => setSelectedFolder('all')}
          rightSection={
            <Badge
              variant='light'
              color='gray'
            >
              {bookmarks.length}
            </Badge>
          }
        />
        <NavLink
          component='div'
          label='Unfiled'
          active={selectedFolder === 'unfiled'}
          onClick={() => setSelectedFolder('unfiled')}
          rightSection={
            <Badge
              variant='light'
              color='gray'
            >
              {unfiledCount}
            </Badge>
          }
        />
        {tree.map((folder) => (
          <FolderNode
            key={folder.id}
            folder={folder}
            selectedFolder={selectedFolder}
            onSelect={setSelectedFolder}
            onRename={openRename}
            onAddChild={openCreate}
            onPublish={(target) => setPublishing(target.id)}
            browsersBySource={browsersBySource}
          />
        ))}
      </div>

      <FolderTargetsModal
        folder={publishingFolder}
        onClose={() => setPublishing(null)}
      />

      <FolderNameModal
        opened={editing !== null}
        onClose={closeModal}
        title={modalTitle}
        initialName={editing?.mode === 'rename' ? editing.folder.name : ''}
        onSubmit={handleSubmit}
      />
    </Stack>
  );
};

export default memo(FolderSidebar);
