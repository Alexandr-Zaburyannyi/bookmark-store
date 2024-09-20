/** @format */

import { useModalStateStore } from '../store/store';
import './Modal.css';
const Modal = (props) => {
  const toggleModal = useModalStateStore((state) => state.toggleModal);
  const modalIsOpen = useModalStateStore((state) => state.modalIsOpen);

  return (
    <div className='modal-container'>
      <button onClick={toggleModal}>Open Modal</button>

      {modalIsOpen && (
        <div className='modal'>
          <div className='modal-content'>{props.children}</div>
        </div>
      )}
    </div>
  );
};
export default Modal;
