/** @format */

import { Button, Flex, Textarea, TextInput } from '@mantine/core';

const BookmarkForm = () => {
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
        <Button type='submit'>Submit</Button>
      </Flex>
    </form>
  );
};
export default BookmarkForm;
