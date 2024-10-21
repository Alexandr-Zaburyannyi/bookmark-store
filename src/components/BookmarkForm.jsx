/** @format */
import { memo } from 'react';
import React, { useMemo } from 'react';
import { Button, Flex, Textarea, TextInput } from '@mantine/core';

const BookmarkForm = () => {
  const memoizedButton = useMemo(
    () => <Button type='submit'>Submit</Button>,
    []
  );

  return (
    <form>
      <Flex
        m='auto'
        w='90%'
        direction='column'
      >
        <TextInput
          my='sm'
          placeholder='Bookamrk name'
          label='Enter bookmark name'
        />
        <TextInput
          my='sm'
          placeholder='Bookmmark link'
          label='Paste bookmark link'
        />
        <Textarea
          my='sm'
          placeholder='Bookmark description'
          label='Enter bookmark description'
        />
        {memoizedButton}
      </Flex>
    </form>
  );
};

export default memo(BookmarkForm);
