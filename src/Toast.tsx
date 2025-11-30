import React from 'react';
import { ToastMessage } from './ToastContext';

interface ToastProps {
  toast: ToastMessage;
  onClose: (id: string) => void;
}

export const Toast: React.FC<ToastProps> = ({ toast, onClose }) => {
  const bgClass = 
    toast.type === 'error' ? 'bg-red-500' : 
    toast.type === 'success' ? 'bg-green-500' : 
    'bg-primary';

  return (
    <div className={`
      mb-3 p-4 rounded-lg shadow-lg text-white min-w-[250px] flex justify-between items-center 
      transform transition-all duration-300 ease-in-out
      ${bgClass}
    `}>
      <span className="text-sm font-medium">{toast.message}</span>
      <button 
        onClick={() => onClose(toast.id)} 
        className="ml-4 text-white/70 hover:text-white transition-colors"
      >
        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
};

interface ToastContainerProps {
  toasts: ToastMessage[];
  removeToast: (id: string) => void;
}

export const ToastContainer: React.FC<ToastContainerProps> = ({ toasts, removeToast }) => {
  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col items-end pointer-events-none">
      <div className="pointer-events-auto">
        {toasts.map((toast) => (
          <Toast key={toast.id} toast={toast} onClose={removeToast} />
        ))}
      </div>
    </div>
  );
};
