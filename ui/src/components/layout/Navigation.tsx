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
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex h-14 sm:h-16 items-center">
          {/* Logo and Navigation */}
          <div className="flex items-center gap-4 sm:gap-6">
            <button 
              onClick={() => onPageChange('dashboard')} 
              className="flex items-center hover:opacity-80 transition-opacity"
            >
              <img
                src="https://nolus.io/favicon/favicon.svg"
                alt="Nolus"
                className="h-6 w-6 sm:h-7 sm:w-7"
              />
            </button>

            <Separator orientation="vertical" className="h-5 sm:h-6" />

            {/* Navigation Links */}
            <nav className="flex items-center gap-0.5 sm:gap-1">
              {navItems.map(({ page, label, icon: Icon }) => (
                <button
                  key={page}
                  onClick={() => onPageChange(page)}
                  className={cn(
                    'flex items-center gap-1.5 sm:gap-2 px-2 sm:px-3 py-1.5 rounded-md text-xs sm:text-sm font-medium transition-colors',
                    'hover:text-foreground',
                    currentPage === page
                      ? 'bg-muted text-foreground'
                      : 'text-muted-foreground'
                  )}
                >
                  <Icon className="h-4 w-4" />
                  <span className="hidden xs:inline sm:inline">{label}</span>
                </button>
              ))}
            </nav>
          </div>
        </div>
      </div>
    </header>
  );
}
