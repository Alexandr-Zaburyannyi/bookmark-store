/** @format */
import { Button, Checkbox, Modal, Stack, Text, Tooltip } from '@mantine/core';
import { memo, useEffect, useState } from 'react';
import useBookmarkStore from '../store/store';
import { loadSources, setFolderTargets } from '../services/bookmarkServices';

const TargetsForm = ({ folder, onClose }) => {
  const sources = useBookmarkStore((state) => state.sources);
  const [selected, setSelected] = useState(() =>
    folder.targets
      .filter((target) => target.state === 'active')
      .map((target) => String(target.sourceId))
  );
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    loadSources();
  }, []);

  const handleSave = async () => {
    setSaving(true);
    const ok = await setFolderTargets(folder.id, selected.map(Number));
    setSaving(false);
    if (ok) {
      onClose();
    }
  };

  return (
    <Stack>
      <Text
        size='sm'
        c='dimmed'
      >
        Choose which browser profiles should receive this folder. Nothing is
        written to a browser until a later sync.
      </Text>
      <Checkbox.Group
        value={selected}
        onChange={setSelected}
      >
        <Stack gap='xs'>
          {sources.map((source) => (
            <Tooltip
              key={source.id}
              label='This profile uses encrypted bookmark storage'
              disabled={source.writability !== 'encrypted'}
            >
              <div>
                <Checkbox
                  value={String(source.id)}
                  label={`${source.browser} · ${source.profile}`}
                  disabled={source.writability === 'encrypted'}
                />
              </div>
            </Tooltip>
          ))}
        </Stack>
      </Checkbox.Group>
      <Button
        loading={saving}
        onClick={handleSave}
      >
        Save
      </Button>
    </Stack>
  );
};

const FolderTargetsModal = ({ folder, onClose }) => (
  <Modal
    opened={folder != null}
    onClose={onClose}
    title={folder ? `Show "${folder.name}" in browsers` : ''}
    radius='md'
    centered
  >
    {folder && (
      <TargetsForm
        key={folder.id}
        folder={folder}
        onClose={onClose}
      />
    )}
  </Modal>
);

export default memo(FolderTargetsModal);
