import React, { useState, useEffect } from 'react';
import { AlertTriangle, Info } from 'lucide-react';
import { SpaceDriver } from '../services/SpaceDriverService';

export const SpaceMouseStatus: React.FC = () => {
  const [isConnected, setIsConnected] = useState(false);
  const [isDismissed, setIsDismissed] = useState(() => {
    // Remember dismissal in sessionStorage so it doesn't re-appear on navigation
    return sessionStorage.getItem('spacemouse-warning-dismissed') === 'true';
  });
  const isSupported = 'hid' in navigator;
  const isSecureContext = window.isSecureContext;
  const isLocalhost = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';
  const currentUrl = window.location.href;

  useEffect(() => {
    const handleConnect = () => setIsConnected(true);
    const handleDisconnect = () => setIsConnected(false);

    SpaceDriver.addEventListener('connect', handleConnect);
    SpaceDriver.addEventListener('disconnect', handleDisconnect);

    return () => {
      SpaceDriver.removeEventListener('connect', handleConnect);
      SpaceDriver.removeEventListener('disconnect', handleDisconnect);
    };
  }, []);


  if (isSupported && isSecureContext) {
    return null;
  }

  // Don't show warning if dismissed or if not relevant
  if (isDismissed) {
    return null;
  }

  const handleDismiss = () => {
    setIsDismissed(true);
    sessionStorage.setItem('spacemouse-warning-dismissed', 'true');
  };

  return (
    <div className="fixed top-4 right-4 z-30 max-w-sm">
      {!isSecureContext && (
        <div className="bg-yellow-900/80 backdrop-blur-sm text-yellow-100 p-3 rounded-lg shadow-lg mb-2 text-xs">
          <div className="flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div className="flex-1">
              <div className="flex items-center justify-between">
                <h3 className="font-semibold text-xs">SpaceMouse Requires Secure Context</h3>
                <button onClick={handleDismiss} className="text-yellow-300 hover:text-white ml-2 text-sm leading-none" title="Dismiss">&times;</button>
              </div>
              <p className="text-[10px] mt-1 opacity-80">
                WebHID needs HTTPS or localhost. Use localhost:3000 or enable insecure origins in chrome://flags.
              </p>
            </div>
          </div>
        </div>
      )}

      {!isSupported && isSecureContext && (
        <div className="bg-blue-900/80 backdrop-blur-sm text-blue-100 p-3 rounded-lg shadow-lg text-xs">
          <div className="flex items-start gap-2">
            <Info className="w-4 h-4 flex-shrink-0 mt-0.5" />
            <div className="flex-1">
              <div className="flex items-center justify-between">
                <h3 className="font-semibold text-xs">WebHID Not Supported</h3>
                <button onClick={handleDismiss} className="text-blue-300 hover:text-white ml-2 text-sm leading-none" title="Dismiss">&times;</button>
              </div>
              <p className="text-[10px] mt-1 opacity-80">SpaceMouse requires Chrome or Edge.</p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};