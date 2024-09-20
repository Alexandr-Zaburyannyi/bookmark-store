/** @format */

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import BookmarkForm from './components/BookmarkForm';
import Modal from './components/Modal';
import './App.css';

function App() {
  const [greetMsg, setGreetMsg] = useState('');
  const [name, setName] = useState('');

  return (
    <div className='container'>
      <h1>Welcome to your bookmark store!</h1>
      <div className='form-container'>
        <Modal>
          <BookmarkForm />
        </Modal>
      </div>
    </div>
  );
}

export default App;
