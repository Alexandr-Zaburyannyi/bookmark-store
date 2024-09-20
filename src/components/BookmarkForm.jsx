/** @format */
import './BookmarkForm.css';

const BookmarkForm = () => {
  return (
    <form className='form'>
      <label>Enter bookmark name</label>
      <input
        type='text'
        placeholder='Bookamrk name'
        className='input'
      />
      <label>Paste bookmark link</label>
      <input
        type='text'
        placeholder='Bookmmark link'
        className='input'
      />
      <label>Enter bookmark description</label>
      <input
        type='text'
        placeholder='Bookmark description'
        className='input'
      />
      <button
        type='submit'
        className='submit-button'
      >
        Submit
      </button>
    </form>
  );
};
export default BookmarkForm;
