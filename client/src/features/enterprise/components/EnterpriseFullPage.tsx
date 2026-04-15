import React from 'react';
import { EnterprisePanel } from './EnterprisePanel';

export function EnterpriseFullPage() {
  return (
    <div className="h-screen w-screen bg-background text-foreground flex flex-col">
      {/* Top bar with back-to-graph button */}
      <header className="flex items-center justify-between px-4 py-2 border-b border-border bg-card/80 backdrop-blur-sm">
        <div className="flex items-center gap-3">
          <a
            href="#/"
            className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="m15 18-6-6 6-6" />
            </svg>
            Back to Graph
          </a>
          <span className="text-border">|</span>
          <h1 className="text-sm font-semibold text-foreground">
            VisionClaw Enterprise
          </h1>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
          Connected
        </div>
      </header>

      {/* Enterprise panel takes remaining space */}
      <div className="flex-1 overflow-hidden">
        <EnterprisePanel />
      </div>
    </div>
  );
}
