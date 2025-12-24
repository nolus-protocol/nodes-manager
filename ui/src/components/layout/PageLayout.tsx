import { ReactNode } from 'react';
import { Navigation } from './Navigation';

interface PageLayoutProps {
  children: ReactNode;
  systemStatus: 'healthy' | 'degraded' | 'critical';
  statusMessage?: string;
}

export function PageLayout({ children, systemStatus, statusMessage }: PageLayoutProps) {
  return (
    <div className="min-h-screen bg-background">
      <Navigation systemStatus={systemStatus} statusMessage={statusMessage} />
      <main className="max-w-7xl mx-auto px-8 py-8">
        {children}
      </main>
    </div>
  );
}
