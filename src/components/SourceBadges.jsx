import { Badge, Group, Tooltip } from '@mantine/core';
import { memo, useMemo } from 'react';
import { groupSources } from '../helpers/helpers';

const SourceBadges = ({ sources }) => {
  const grouped = useMemo(() => groupSources(sources), [sources]);

  if (grouped.length === 0) {
    return null;
  }

  return (
    <Group gap='xs'>
      {grouped.map((source) => (
        <Tooltip
          key={source.sourceId}
          label={source.folders.join(', ')}
        >
          <Badge
            variant={source.present ? 'light' : 'outline'}
            c={source.present ? undefined : 'dimmed'}
            td={source.present ? undefined : 'line-through'}
          >
            {source.label}
          </Badge>
        </Tooltip>
      ))}
    </Group>
  );
};

export default memo(SourceBadges);
