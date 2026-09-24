import { Text } from '@mantine/core';
import { memo } from 'react';
import useBookmarkStore from '../store/store';

const SyncStatus = () => {
  const syncing = useBookmarkStore((state) => state.syncing);
  const lastSync = useBookmarkStore((state) => state.lastSync);

  if (syncing) {
    return (
      <Text
        size='sm'
        c='dimmed'
      >
        Syncing…
      </Text>
    );
  }

  if (!lastSync) {
    return null;
  }

  return (
    <Text
      size='sm'
      c='dimmed'
    >
      Last synced {new Date(lastSync.at).toLocaleTimeString()}
    </Text>
  );
};

export default memo(SyncStatus);
