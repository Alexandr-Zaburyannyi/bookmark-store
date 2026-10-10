/** @format */
import { memo, useEffect, useRef, useState } from 'react';
import { CloseButton, TextInput } from '@mantine/core';
import { useDebouncedValue, useHotkeys } from '@mantine/hooks';
import useBookmarkStore from '../store/store';

const SearchInput = () => {
  const setSearchQuery = useBookmarkStore((state) => state.setSearchQuery);
  const [value, setValue] = useState('');
  const [debounced] = useDebouncedValue(value, 150);
  const inputRef = useRef(null);

  useEffect(() => {
    setSearchQuery(debounced);
  }, [debounced, setSearchQuery]);

  useHotkeys([['mod+K', () => inputRef.current?.focus()]]);

  const clear = () => {
    setValue('');
    setSearchQuery('');
  };

  return (
    <TextInput
      ref={inputRef}
      w={280}
      placeholder='Search bookmarks  (Ctrl+K)'
      value={value}
      onChange={(event) => setValue(event.currentTarget.value)}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          clear();
        }
      }}
      rightSection={
        value ? (
          <CloseButton
            size='sm'
            aria-label='Clear search'
            onClick={clear}
          />
        ) : null
      }
    />
  );
};

export default memo(SearchInput);
