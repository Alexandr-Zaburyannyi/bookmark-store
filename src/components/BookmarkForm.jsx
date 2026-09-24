import React, { useMemo, memo } from 'react';
import { Button, Flex, Textarea, TextInput } from '@mantine/core';

const BookmarkForm = () => {
  const handleSubmit = (e) => {
    e.preventDefault();
  };
  // use react hook form for this form & possibly special form component
  const memoizedButton = useMemo(
    () => (
      <Button
        type='submit'
        onClick={handleSubmit}
      >
        Submit
      </Button>
    ),
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
