import { Badge, Drawer, Group, Stack, Text } from '@mantine/core';
import { memo, useEffect } from 'react';
import useBookmarkStore from '../store/store';
import { loadSources } from '../services/bookmarkServices';

const formatLastSynced = (lastSyncedAt) =>
  lastSyncedAt ? new Date(lastSyncedAt).toLocaleString() : 'Never';

const WRITABILITY_BADGE = {
  writable: () => ({ color: 'green', text: 'Writable' }),
  browser_running: (browser) => ({
    color: 'yellow',
    text: `Close ${browser} to write`,
  }),
  encrypted: () => ({ color: 'gray', text: 'Read-only: encrypted storage' }),
  missing: () => ({ color: 'gray', text: 'Bookmarks file missing' }),
};

const WritabilityBadge = ({ source }) => {
  const describe = WRITABILITY_BADGE[source.writability];
  if (!describe) {
    return null;
  }
  const { color, text } = describe(source.browser);
  return (
    <Badge
      variant='light'
      color={color}
      w='fit-content'
    >
      {text}
    </Badge>
  );
};

const SourceRow = ({ source }) => (
  <Stack
    gap={4}
    py='xs'
  >
    <Group justify='space-between'>
      <Text fw={500}>
        {source.browser} · {source.profile}
      </Text>
      <Badge variant='light'>{source.presentCount}</Badge>
    </Group>
    <WritabilityBadge source={source} />
    {source.pendingCount > 0 && (
      <Text size='sm'>
        {source.pendingCount} pending{' '}
        {source.pendingCount === 1 ? 'change' : 'changes'}
      </Text>
    )}
    <Text
      size='sm'
      c='dimmed'
    >
      Last synced: {formatLastSynced(source.lastSyncedAt)}
    </Text>
    {source.lastError && (
      <Text
        size='sm'
        c='red'
      >
        {source.lastError}
      </Text>
    )}
  </Stack>
);

const SourcesDrawer = ({ opened, onClose }) => {
  const sources = useBookmarkStore((state) => state.sources);

  useEffect(() => {
    if (opened) {
      loadSources();
    }
  }, [opened]);

  return (
    <Drawer
      opened={opened}
      onClose={onClose}
      title='Sources'
      position='right'
    >
      <Stack gap={0}>
        {sources.length === 0 && (
          <Text
            size='sm'
            c='dimmed'
          >
            No browser profiles detected yet.
          </Text>
        )}
        {sources.map((source) => (
          <SourceRow
            key={source.id}
            source={source}
          />
        ))}
      </Stack>
    </Drawer>
  );
};

export default memo(SourcesDrawer);
