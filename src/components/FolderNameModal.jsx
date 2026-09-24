/** @format */
import { Button, Modal, Stack, TextInput } from '@mantine/core';
import { useForm } from '@mantine/form';
import { memo, useEffect } from 'react';

const FolderNameModal = ({ opened, onClose, title, initialName, onSubmit }) => {
  const form = useForm({
    initialValues: { name: initialName },
    validate: {
      name: (value) => (value.trim() ? null : 'Enter a folder name'),
    },
  });

  useEffect(() => {
    if (opened) {
      form.setValues({ name: initialName });
      form.resetDirty();
      form.clearErrors();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, initialName]);

  return (
    <Modal
      title={title}
      opened={opened}
      onClose={onClose}
      radius='md'
      centered
    >
      <form
        onSubmit={form.onSubmit(({ name }) => {
          onSubmit(name.trim());
        })}
      >
        <Stack>
          <TextInput
            label='Name'
            data-autofocus
            {...form.getInputProps('name')}
          />
          <Button type='submit'>Save</Button>
        </Stack>
      </form>
    </Modal>
  );
};

export default memo(FolderNameModal);
