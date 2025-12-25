import {
  Separator,
  cn,
} from '@kostovster/ui';
import { LayoutDashboard, Boxes, Server } from 'lucide-react';

type Page = 'dashboard' | 'nodes' | 'services';

interface NavigationProps {
  currentPage: Page;
  onPageChange: (page: Page) => void;
}

const navItems: { page: Page; label: string; icon: typeof LayoutDashboard }[] = [
  { page: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { page: 'nodes', label: 'Nodes', icon: Boxes },
  { page: 'services', label: 'Services', icon: Server },
];

export function Navigation({ currentPage, onPageChange }: NavigationProps) {
  return (
    <header className="border-b bg-card">
      <div className="max-w-7xl mx-auto px-8">
        <div className="flex h-16 items-center">
          {/* Logo and Navigation */}
          <div className="flex items-center gap-6">
            <button 
              onClick={() => onPageChange('dashboard')} 
              className="flex items-center hover:opacity-80 transition-opacity"
            >
              <img
                src="https://nolus.io/favicon/favicon.svg"
                alt="Nolus"
                className="h-7 w-7"
              />
            </button>

            <Separator orientation="vertical" className="h-6" />

            {/* Navigation Links */}
            <nav className="flex items-center gap-1">
              {navItems.map(({ page, label, icon: Icon }) => (
                <button
                  key={page}
                  onClick={() => onPageChange(page)}
                  className={cn(
                    'flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium transition-colors',
                    'hover:text-foreground',
                    currentPage === page
                      ? 'bg-muted text-foreground'
                      : 'text-muted-foreground'
                  )}
                >
                  <Icon className="h-4 w-4" />
                  {label}
                </button>
              ))}
            </nav>
          </div>
        </div>
      </div>
    </header>
  );
}
