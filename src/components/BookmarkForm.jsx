/** @format */
import { memo, useMemo, useState } from 'react';
import { Button, Flex, Select, Textarea, TextInput } from '@mantine/core';
import { useForm } from '@mantine/form';
import useBookmarkStore from '../store/store';
import { addBookmark } from '../services/bookmarkServices';
import {
  displayHost,
  folderOptions,
  normalizeLink,
} from '../helpers/helpers';

const BookmarkForm = ({ onDone }) => {
  const folders = useBookmarkStore((state) => state.folders);
  const selectedFolder = useBookmarkStore((state) => state.selectedFolder);
  const [saving, setSaving] = useState(false);

  const form = useForm({
    initialValues: {
      link: '',
      name: '',
      description: '',
      folder:
        typeof selectedFolder === 'number' ? String(selectedFolder) : 'unfiled',
    },
    validate: {
      link: (value) => {
        if (!value.trim()) {
          return 'Link is required';
        }
        return normalizeLink(value) ? null : 'Enter a valid link';
      },
    },
  });

  const folderData = useMemo(
    () => [{ value: 'unfiled', label: 'Unfiled' }, ...folderOptions(folders)],
    [folders]
  );

  const handleSubmit = async (values) => {
    const link = normalizeLink(values.link);

    setSaving(true);
    const result = await addBookmark({
      name: values.name.trim() || displayHost(link),
      link,
      description: values.description.trim(),
      folderId: values.folder === 'unfiled' ? null : Number(values.folder),
    });
    setSaving(false);

    if (!result) {
      return;
    }
    if (result.outcome === 'exists') {
      form.setFieldError('link', `Already saved as “${result.name}”`);
      return;
    }

    form.reset();
    onDone();
  };

  const memoizedButton = useMemo(
    () => (
      <Button
        type='submit'
        loading={saving}
      >
        Save
      </Button>
    ),
    [saving]
  );

  return (
    <form onSubmit={form.onSubmit(handleSubmit)}>
      <Flex
        m='auto'
        w='90%'
        direction='column'
      >
        <TextInput
          my='sm'
          data-autofocus
          label='Link'
          placeholder='https://example.com'
          {...form.getInputProps('link')}
        />
        <TextInput
          my='sm'
          label='Name'
          placeholder="Defaults to the site's host"
          {...form.getInputProps('name')}
        />
        <Textarea
          my='sm'
          label='Description'
          autosize
          minRows={2}
          {...form.getInputProps('description')}
        />
        <Select
          my='sm'
          label='Folder'
          data={folderData}
          allowDeselect={false}
          comboboxProps={{ withinPortal: false }}
          {...form.getInputProps('folder')}
        />
        {memoizedButton}
      </Flex>
    </form>
  );
};

export default memo(BookmarkForm);
