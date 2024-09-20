/** @format */

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import './App.css';

function App() {
  const [greetMsg, setGreetMsg] = useState('');
  const [name, setName] = useState('');

  return (
    <div className='container'>
      <h1>Welcome to your bookmark store!</h1>

      <div className='row'></div>

      <p>Click on the Tauri, Vite, and React logos to learn more.</p>
    </div>
  );
}

export default App;
