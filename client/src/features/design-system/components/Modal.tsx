import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from './Dialog';

export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title?: string;
  children: React.ReactNode;
  size?: 'small' | 'medium' | 'large' | 'full';
  closeOnOverlayClick?: boolean;
  showCloseButton?: boolean;
  className?: string;
}

const sizeClasses: Record<NonNullable<ModalProps['size']>, string> = {
  small: 'sm:max-w-md',
  medium: 'sm:max-w-2xl',
  large: 'sm:max-w-4xl',
  full: 'sm:max-w-[calc(100vw-2rem)]',
};

/**
 * Modal — wraps the Radix-based Dialog component for backward compatibility.
 * Prefer using Dialog/DialogContent directly for new code.
 */
export const Modal: React.FC<ModalProps> = ({
  isOpen,
  onClose,
  title,
  children,
  size = 'medium',
  closeOnOverlayClick = true,
  showCloseButton: _showCloseButton = true,
  className = '',
}) => {
  const handleOpenChange = (open: boolean) => {
    if (!open) onClose();
  };

  return (
    <Dialog open={isOpen} onOpenChange={handleOpenChange}>
      <DialogContent
        className={`${sizeClasses[size]} ${className}`}
        onPointerDownOutside={
          closeOnOverlayClick ? undefined : (e) => e.preventDefault()
        }
        onEscapeKeyDown={undefined}
      >
        {title && (
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
        )}
        <div>{children}</div>
      </DialogContent>
    </Dialog>
  );
};

// Compound components using design tokens
export interface ModalBodyProps {
  children: React.ReactNode;
  className?: string;
}

export const ModalBody: React.FC<ModalBodyProps> = ({ children, className = '' }) => (
  <div className={`text-muted-foreground ${className}`}>
    {children}
  </div>
);

export interface ModalFooterProps {
  children: React.ReactNode;
  className?: string;
}

export const ModalFooter: React.FC<ModalFooterProps> = ({ children, className = '' }) => (
  <DialogFooter className={className}>
    {children}
  </DialogFooter>
);

// Confirmation Modal using design tokens
export interface ConfirmModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  confirmButtonClassName?: string;
  isDestructive?: boolean;
}

export const ConfirmModal: React.FC<ConfirmModalProps> = ({
  isOpen,
  onClose,
  onConfirm,
  title,
  message,
  confirmText = 'Confirm',
  cancelText = 'Cancel',
  confirmButtonClassName,
  isDestructive = false,
}) => {
  const handleConfirm = () => {
    onConfirm();
    onClose();
  };

  const defaultConfirmClass = isDestructive
    ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
    : 'bg-primary text-primary-foreground hover:bg-primary/90';

  return (
    <Modal isOpen={isOpen} onClose={onClose} title={title} size="small">
      <ModalBody>
        <DialogDescription>{message}</DialogDescription>
      </ModalBody>
      <ModalFooter>
        <button
          type="button"
          className="rounded-lg border border-border bg-background px-4 py-2 text-foreground hover:bg-accent"
          onClick={onClose}
        >
          {cancelText}
        </button>
        <button
          type="button"
          className={confirmButtonClassName || `rounded-lg px-4 py-2 ${defaultConfirmClass}`}
          onClick={handleConfirm}
        >
          {confirmText}
        </button>
      </ModalFooter>
    </Modal>
  );
};
